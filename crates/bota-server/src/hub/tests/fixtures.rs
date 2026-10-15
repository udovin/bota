//! Helpers shared by the hub tests: a hub on a free port with a data
//! directory of its own, and a client that speaks frames.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use bota_proto::{
    AccountInfo, Auth, ClientMsg, FrameReader, HeroId, MapId, MatchId, MatchState, ModeId,
    ServerMsg, SlotId, TickMode, Token, encode_frame_to_vec,
};

use crate::hub::{HashCost, HubOpts, Mode, serve};

/// The realtime test mode: a 1v1 on the Dota map at sixty ticks a second.
pub const REALTIME: ModeId = ModeId(0);
/// The lockstep test mode: only acknowledgements move it.
pub const LOCKSTEP: ModeId = ModeId(1);
/// The four-seat test mode, whose cups have teams of two.
pub const TWO_A_SIDE: ModeId = ModeId(2);

/// What came of asking a client for its next message.
pub enum Heard {
    /// One message.
    Msg(ServerMsg),
    /// The read timed out with nothing to show.
    Quiet,
    /// The server hung up.
    Closed,
}

/// Whether a read error means the socket went quiet rather than broke.
fn quiet(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

pub struct TestClient {
    stream: TcpStream,
    reader: FrameReader,
}

impl TestClient {
    pub fn connect(addr: SocketAddr) -> TestClient {
        let stream = TcpStream::connect(addr).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set timeout");
        TestClient {
            stream,
            reader: FrameReader::new(),
        }
    }

    pub fn send(&mut self, msg: &ClientMsg) {
        let frame = encode_frame_to_vec(msg).expect("encode");
        self.stream.write_all(&frame).expect("send");
    }

    /// The next message, or why there is none.
    pub fn heard(&mut self) -> Heard {
        let mut buf = [0u8; 65536];
        loop {
            if let Some(msg) = self.reader.next_message::<ServerMsg>().expect("decode") {
                return Heard::Msg(msg);
            }
            match self.stream.read(&mut buf) {
                Ok(0) => return Heard::Closed,
                Ok(n) => self.reader.push(&buf[..n]),
                Err(err) if quiet(&err) => return Heard::Quiet,
                Err(_) => return Heard::Closed,
            }
        }
    }

    pub fn recv(&mut self) -> ServerMsg {
        match self.heard() {
            Heard::Msg(msg) => msg,
            Heard::Closed => panic!("the server hung up while a message was awaited"),
            Heard::Quiet => panic!("nothing arrived before the read timed out"),
        }
    }

    pub fn recv_until<T>(&mut self, mut pick: impl FnMut(ServerMsg) -> Option<T>) -> T {
        for _ in 0..600 {
            if let Some(found) = pick(self.recv()) {
                return found;
            }
        }
        panic!("the awaited message never came");
    }

    /// Reads until the server hangs up. Returns whether it did before the
    /// read timed out.
    pub fn hung_up(&mut self) -> bool {
        for _ in 0..600 {
            match self.heard() {
                Heard::Msg(_) => {}
                Heard::Closed => return true,
                Heard::Quiet => return false,
            }
        }
        false
    }

    pub fn match_state(&mut self) -> MatchState {
        self.recv_until(|msg| match msg {
            ServerMsg::MatchState { state } => Some(state),
            _ => None,
        })
    }

    pub fn match_start(&mut self) -> bota_proto::MatchInfo {
        self.recv_until(|msg| match msg {
            ServerMsg::MatchStart { info } => Some(info),
            _ => None,
        })
    }

    pub fn hall(&mut self) -> Vec<bota_proto::MatchSummary> {
        self.recv_until(|msg| match msg {
            ServerMsg::Hall { matches, .. } => Some(matches),
            _ => None,
        })
    }

    pub fn refusal(&mut self) -> bota_proto::HallReason {
        self.recv_until(|msg| match msg {
            ServerMsg::Refused { reason } => Some(reason),
            _ => None,
        })
    }

    pub fn snapshot(&mut self) -> bota_proto::WorldView {
        self.recv_until(|msg| match msg {
            ServerMsg::Snapshot { view } => Some(view),
            _ => None,
        })
    }

    pub fn pick_and_ready(&mut self) {
        self.send(&ClientMsg::PickHero { hero: HeroId(0) });
        self.send(&ClientMsg::SetReady(true));
    }
}

static DIRS: AtomicU32 = AtomicU32::new(0);

/// A data directory nobody else uses.
pub fn fresh_dir(tag: &str) -> PathBuf {
    let n = DIRS.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "bota-hub-{}-{}-{n}-{tag}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// Options for a hub under test: cheap hashing, no hall throttling, short
/// timeouts.
pub fn test_opts(data: PathBuf) -> HubOpts {
    let mode = |id: u16, name: &str, tick: TickMode, seats: u8| Mode {
        id: ModeId(id),
        name: name.to_string(),
        map: MapId(0),
        seats,
        tick,
        tick_rate: 60,
        ack_timeout_ticks: 600,
        cheats: false,
    };
    HubOpts {
        data,
        modes: vec![
            mode(0, "test realtime", TickMode::Realtime, 2),
            mode(1, "test lockstep", TickMode::Lockstep, 2),
            mode(2, "test 2v2", TickMode::Realtime, 4),
        ],
        cheats: false,
        hash_cost: HashCost::Light,
        token_life: Duration::from_secs(3600),
        abandon_after: Duration::from_millis(200),
        ready_timeout: Duration::from_millis(400),
        empty_ttl: Duration::from_secs(600),
        over_ttl: Duration::from_secs(600),
        hall_interval: Duration::ZERO,
    }
}

/// Starts a hub on a free port and returns where it listens.
pub fn start_hub(opts: HubOpts) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    thread::spawn(move || serve(listener, opts));
    addr
}

/// A hub with a fresh directory.
pub fn hub(tag: &str) -> (SocketAddr, PathBuf) {
    let dir = fresh_dir(tag);
    (start_hub(test_opts(dir.clone())), dir)
}

/// Registers an account and reads its welcome.
pub fn sign_up(addr: SocketAddr, login: &str) -> (TestClient, AccountInfo, Token) {
    let mut client = TestClient::connect(addr);
    client.send(&ClientMsg::Hello {
        auth: Auth::Register {
            login: login.to_string(),
            password: format!("{login}-pw"),
        },
    });
    let (account, token) = client.recv_until(|msg| match msg {
        ServerMsg::Welcome { account, token, .. } => Some((account, token)),
        _ => None,
    });
    (
        client,
        account.expect("an account"),
        token.expect("a token"),
    )
}

/// Two accounts in one gathering match, the host on seat zero.
pub fn gathered(addr: SocketAddr, mode: ModeId) -> (TestClient, TestClient, MatchId) {
    let (mut host, _, _) = sign_up(addr, "alpha");
    let (mut guest, _, _) = sign_up(addr, "beta");
    host.send(&ClientMsg::CreateMatch {
        mode,
        name: "test".to_string(),
        seed: Some(7),
    });
    let state = host.match_state();
    assert_eq!(state.you, Some(SlotId(0)));
    guest.send(&ClientMsg::JoinMatch {
        id: state.id,
        role: bota_proto::Role::Player,
    });
    let seen = guest.match_state();
    assert_eq!(seen.you, Some(SlotId(1)));
    (host, guest, state.id)
}

/// Two accounts in one match that has started.
pub fn started(addr: SocketAddr, mode: ModeId) -> (TestClient, TestClient, MatchId) {
    let (mut host, mut guest, id) = gathered(addr, mode);
    host.pick_and_ready();
    guest.pick_and_ready();
    host.match_start();
    guest.match_start();
    (host, guest, id)
}
