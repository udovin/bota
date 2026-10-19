//! Helpers shared by the hub tests: a hub and a game server on free ports
//! with a data directory of their own, and a client that speaks frames to
//! both.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use bota_proto::{
    AccountInfo, Auth, EntityId, FrameReader, FromGame, FromHub, HeroId, MapId, MatchId,
    MatchState, Message, ModeId, Order, PickReason, PickState, SlotId, Team, TickMode, Ticket,
    ToGame, ToHub, Token, UnitOrder, encode_frame_to_vec,
};

use crate::game::STANDARD_RULES;
use crate::host::{HostOpts, host};
use crate::hub::{HashCost, HubOpts, Mode, serve};
use crate::keys::fresh;

/// The realtime test mode: a 1v1 on the Dota map at sixty ticks a second.
pub const REALTIME: ModeId = ModeId(0);
/// The lockstep test mode: only acknowledgements move it.
pub const LOCKSTEP: ModeId = ModeId(1);
/// The four-seat test mode, whose cups have teams of two: one seat to an
/// account, as in Dota.
pub const TWO_A_SIDE: ModeId = ModeId(2);
/// The four-seat lockstep test mode, an arena: an account may drive a whole
/// side.
pub const CREW_LOCKSTEP: ModeId = ModeId(3);
/// The realtime test mode with cheats on and spectators let to write.
pub const SANDBOX: ModeId = ModeId(4);

/// How long a test game server waits for a ticket nobody admitted.
pub const TEST_TICKET_GRACE: Duration = Duration::from_millis(300);

/// Most messages read from a game server while waiting for one.
const GAME_READS: usize = 20_000;

/// What came of asking a socket for its next message.
pub enum Heard<M> {
    /// One message.
    Msg(M),
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

/// One socket and what has been read off it.
pub struct Line {
    stream: TcpStream,
    reader: FrameReader,
}

impl Line {
    /// Connects, with reads that give up after ten seconds.
    pub fn open(addr: &str) -> Line {
        let stream = TcpStream::connect(addr).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set timeout");
        Line {
            stream,
            reader: FrameReader::new(),
        }
    }

    /// Writes one message.
    pub fn send<T: Message>(&mut self, msg: &T) {
        let frame = encode_frame_to_vec(msg).expect("encode");
        self.stream.write_all(&frame).expect("send");
    }

    /// The next message, or why there is none.
    pub fn heard<M: Message>(&mut self) -> Heard<M> {
        let mut buf = [0u8; 65536];
        loop {
            if let Some(msg) = self.reader.next_message::<M>().expect("decode") {
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
}

/// A participant: its line to the hub, and its line to the game server its
/// match plays on once it has gone there.
pub struct TestClient {
    hub: Line,
    game: Option<Line>,
    /// Where the hub last sent this connection to play, and the ticket.
    pub play: Option<(MatchId, String, Ticket)>,
    /// The seats the last match state said this connection holds.
    pub yours: Vec<SlotId>,
}

impl TestClient {
    pub fn connect(addr: SocketAddr) -> TestClient {
        TestClient {
            hub: Line::open(&addr.to_string()),
            game: None,
            play: None,
            yours: Vec::new(),
        }
    }

    pub fn send(&mut self, msg: &ToHub) {
        self.hub.send(msg);
    }

    /// The next message from the hub, or why there is none. Keeps the seats
    /// a match state names and where a [`FromHub::Play`] sends, and a play
    /// in another match than the last lets go of the last one's game server.
    pub fn heard(&mut self) -> Heard<FromHub> {
        let heard = self.hub.heard::<FromHub>();
        match &heard {
            Heard::Msg(FromHub::MatchState { state }) => self.yours = state.yours.clone(),
            Heard::Msg(FromHub::Play {
                match_id,
                addr,
                ticket,
            }) => {
                if self.play.as_ref().map(|(id, _, _)| *id) != Some(*match_id) {
                    self.game = None;
                }
                self.play = Some((*match_id, addr.clone(), *ticket));
            }
            _ => {}
        }
        heard
    }

    pub fn recv(&mut self) -> FromHub {
        match self.heard() {
            Heard::Msg(msg) => msg,
            Heard::Closed => panic!("the hub hung up while a message was awaited"),
            Heard::Quiet => panic!("nothing arrived from the hub before the read timed out"),
        }
    }

    pub fn recv_until<T>(&mut self, mut pick: impl FnMut(FromHub) -> Option<T>) -> T {
        for _ in 0..600 {
            if let Some(found) = pick(self.recv()) {
                return found;
            }
        }
        panic!("the awaited message never came");
    }

    /// Reads the hub until it hangs up. Returns whether it did before the
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

    /// Goes where the hub sent this connection to play, waiting for the hub
    /// to say where when it has not yet, and joins with the ticket.
    pub fn enter_game(&mut self) {
        if self.play.is_none() {
            self.recv_until(|msg| matches!(msg, FromHub::Play { .. }).then_some(()));
        }
        let (_, addr, ticket) = self.play.clone().expect("a place to play");
        let mut line = Line::open(&addr);
        line.send(&ToGame::Join { ticket });
        self.game = Some(line);
    }

    /// Lets go of the game server, keeping the ticket to come back with.
    pub fn leave_game(&mut self) {
        self.game = None;
    }

    fn game_line(&mut self) -> &mut Line {
        if self.game.is_none() {
            self.enter_game();
        }
        self.game.as_mut().expect("joined")
    }

    pub fn send_game(&mut self, msg: &ToGame) {
        self.game_line().send(msg);
    }

    /// The next message from the game server, going there first if need be.
    pub fn game_heard(&mut self) -> Heard<FromGame> {
        self.game_line().heard::<FromGame>()
    }

    pub fn game_recv(&mut self) -> FromGame {
        match self.game_heard() {
            Heard::Msg(msg) => msg,
            Heard::Closed => panic!("the game server hung up while a message was awaited"),
            Heard::Quiet => panic!("nothing arrived from the game before the read timed out"),
        }
    }

    /// Reads the game server until a message gives something. A match sends
    /// two messages a tick, so far more are read through than from the hub.
    pub fn game_recv_until<T>(&mut self, mut pick: impl FnMut(FromGame) -> Option<T>) -> T {
        for _ in 0..GAME_READS {
            if let Some(found) = pick(self.game_recv()) {
                return found;
            }
        }
        panic!("the awaited game message never came");
    }

    /// Reads the game server until it hangs up. Returns whether it did
    /// before the read timed out.
    pub fn game_hung_up(&mut self) -> bool {
        for _ in 0..GAME_READS {
            match self.game_heard() {
                Heard::Msg(_) => {}
                Heard::Closed => return true,
                Heard::Quiet => return false,
            }
        }
        false
    }

    /// Orders units, one order each, in one message. Returns them as given.
    pub fn order(&mut self, orders: &[(EntityId, Order)]) -> Vec<UnitOrder> {
        let given: Vec<UnitOrder> = orders
            .iter()
            .map(|(unit, order)| UnitOrder {
                unit: *unit,
                order: *order,
            })
            .collect();
        self.send_game(&ToGame::Orders {
            orders: given.clone(),
        });
        given
    }

    /// The next refusal: the order as it was given, and why.
    pub fn order_refused(&mut self) -> (UnitOrder, bota_proto::RejectReason) {
        self.game_recv_until(|msg| match msg {
            FromGame::OrderRejected { order, reason } => Some((order, reason)),
            _ => None,
        })
    }

    pub fn match_state(&mut self) -> MatchState {
        self.recv_until(|msg| match msg {
            FromHub::MatchState { state } => Some(state),
            _ => None,
        })
    }

    /// The start of the match, on its game server.
    pub fn match_start(&mut self) -> bota_proto::MatchInfo {
        self.game_recv_until(|msg| match msg {
            FromGame::MatchStart { info } => Some(info),
            _ => None,
        })
    }

    /// The next word on where this connection stands in matchmaking.
    pub fn search(&mut self) -> bota_proto::Search {
        self.recv_until(|msg| match msg {
            FromHub::Search { search } => Some(search),
            _ => None,
        })
    }

    /// Reads word on matchmaking until a match is found for this connection.
    /// Returns its id.
    pub fn found(&mut self) -> MatchId {
        self.recv_until(|msg| match msg {
            FromHub::Search {
                search: bota_proto::Search::Found { id, .. },
            } => Some(id),
            _ => None,
        })
    }

    pub fn hall(&mut self) -> Vec<bota_proto::MatchSummary> {
        self.recv_until(|msg| match msg {
            FromHub::Hall { matches, .. } => Some(matches),
            _ => None,
        })
    }

    pub fn refusal(&mut self) -> bota_proto::HallReason {
        self.recv_until(|msg| match msg {
            FromHub::Refused { reason } => Some(reason),
            _ => None,
        })
    }

    pub fn snapshot(&mut self) -> bota_proto::WorldView {
        self.game_recv_until(|msg| match msg {
            FromGame::Snapshot { view } => Some(view),
            _ => None,
        })
    }

    /// Says a line in the hall or the lobby, or types a command there.
    pub fn chat(&mut self, text: &str, team: bool) {
        self.send(&ToHub::Chat {
            text: text.to_string(),
            team,
        });
    }

    /// Says a line in the running match, or types a command there.
    pub fn chat_game(&mut self, text: &str, team: bool) {
        self.send_game(&ToGame::Chat {
            text: text.to_string(),
            team,
        });
    }

    /// The next line said in the hall or the lobby: who said it, on which
    /// side, whether to that side alone, and what.
    pub fn said(&mut self) -> (String, Option<Team>, bool, String) {
        self.recv_until(|msg| match msg {
            FromHub::Chat {
                from,
                side,
                team,
                text,
            } => Some((from, side, team, text)),
            _ => None,
        })
    }

    /// The next line said in the running match.
    pub fn game_said(&mut self) -> (String, Option<Team>, bool, String) {
        self.game_recv_until(|msg| match msg {
            FromGame::Chat {
                from,
                side,
                team,
                text,
            } => Some((from, side, team, text)),
            _ => None,
        })
    }

    /// The next line from the hub itself.
    pub fn notice(&mut self) -> String {
        self.recv_until(|msg| match msg {
            FromHub::Notice { text } => Some(text),
            _ => None,
        })
    }

    /// The next line from the game server itself.
    pub fn game_notice(&mut self) -> String {
        self.game_recv_until(|msg| match msg {
            FromGame::Notice { text } => Some(text),
            _ => None,
        })
    }

    /// The next word on how the picking of heroes stands, from the game
    /// server, going there first if need be.
    pub fn picking(&mut self) -> PickState {
        self.game_recv_until(|msg| match msg {
            FromGame::Picking { state } => Some(state),
            _ => None,
        })
    }

    /// Picks one hero for every seat held, once the game server says heroes
    /// are being picked.
    pub fn pick(&mut self, hero: HeroId) {
        for seat in self.picking().yours {
            self.send_game(&ToGame::Pick { seat, hero });
        }
    }

    /// The next pick turned down: the seat, the hero, and why.
    pub fn pick_refused(&mut self) -> (SlotId, HeroId, PickReason) {
        self.game_recv_until(|msg| match msg {
            FromGame::PickRefused { seat, hero, reason } => Some((seat, hero, reason)),
            _ => None,
        })
    }
}

/// The hero standing for a seat in a view.
pub fn hero_of(view: &bota_proto::WorldView, slot: SlotId) -> EntityId {
    view.players
        .iter()
        .find(|p| p.slot == slot)
        .and_then(|p| p.unit)
        .expect("a standing hero")
}

/// Where a unit stands in a view.
pub fn where_is(view: &bota_proto::WorldView, unit: EntityId) -> bota_proto::Vec2 {
    view.units
        .iter()
        .find(|u| u.id == unit)
        .map(|u| u.pos)
        .expect("a unit in the view")
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
/// timeouts, a key of its own.
pub fn test_opts(data: PathBuf) -> HubOpts {
    let mode = |id: u16, name: &str, tick: TickMode, seats: u8| Mode {
        id: ModeId(id),
        name: name.to_string(),
        map: MapId(0),
        seats,
        seats_per_player: 1,
        tick,
        tick_rate: 60,
        ack_timeout_ticks: 600,
        rules: STANDARD_RULES,
        spectator_chat: false,
    };
    HubOpts {
        data,
        key: fresh(),
        modes: vec![
            mode(0, "test realtime", TickMode::Realtime, 2),
            mode(1, "test lockstep", TickMode::Lockstep, 2),
            mode(2, "test 2v2", TickMode::Realtime, 4),
            Mode {
                seats_per_player: 2,
                ..mode(3, "test 2v2 lockstep", TickMode::Lockstep, 4)
            },
            Mode {
                rules: bota_proto::Rules {
                    cheats: true,
                    ..STANDARD_RULES
                },
                spectator_chat: true,
                ..mode(4, "test sandbox", TickMode::Realtime, 2)
            },
        ],
        cheats: false,
        hash_cost: HashCost::Light,
        token_life: Duration::from_secs(3600),
        abandon_after: Duration::from_millis(200),
        ready_timeout: Duration::from_millis(400),
        accept_timeout: Duration::from_millis(1500),
        pick_timeout: Duration::from_millis(1500),
        empty_ttl: Duration::from_secs(600),
        over_ttl: Duration::from_secs(600),
        hall_interval: Duration::ZERO,
    }
}

/// Where a hub under test listens.
pub struct Ports {
    /// Players reach the hub here.
    pub hub: SocketAddr,
    /// Game servers reach the hub here.
    pub servers: SocketAddr,
}

/// Starts a hub on free ports, with no game server yet.
pub fn start_hub_alone(opts: HubOpts) -> Ports {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let servers = TcpListener::bind("127.0.0.1:0").expect("bind");
    let ports = Ports {
        hub: listener.local_addr().expect("addr"),
        servers: servers.local_addr().expect("addr"),
    };
    thread::spawn(move || serve(listener, servers, opts));
    ports
}

/// Starts a game server on a free port that offers itself to the hub with
/// a key. Returns where players reach it.
pub fn start_game_server(servers: SocketAddr, key: [u8; 32]) -> SocketAddr {
    let players = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = players.local_addr().expect("addr");
    let opts = HostOpts {
        hub: servers.to_string(),
        key,
        public: addr.to_string(),
        capacity: 16,
        ticket_grace: TEST_TICKET_GRACE,
    };
    thread::spawn(move || host(players, opts));
    addr
}

/// Starts a hub and one game server for it, and returns where players
/// reach the hub.
pub fn start_hub(opts: HubOpts) -> SocketAddr {
    let key = opts.key;
    let ports = start_hub_alone(opts);
    start_game_server(ports.servers, key);
    ports.hub
}

/// A hub with a fresh directory and a game server.
pub fn hub(tag: &str) -> (SocketAddr, PathBuf) {
    let dir = fresh_dir(tag);
    (start_hub(test_opts(dir.clone())), dir)
}

/// Registers an account and reads its welcome.
pub fn sign_up(addr: SocketAddr, login: &str) -> (TestClient, AccountInfo, Token) {
    let mut client = TestClient::connect(addr);
    client.send(&ToHub::Hello {
        auth: Auth::Register {
            login: login.to_string(),
            password: format!("{login}-pw"),
        },
    });
    let (account, token) = client.recv_until(|msg| match msg {
        FromHub::Welcome { account, token, .. } => Some((account, token)),
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
    host.send(&ToHub::CreateMatch {
        mode,
        name: "test".to_string(),
        seed: Some(7),
        seats: 1,
    });
    let state = host.match_state();
    assert_eq!(state.yours.first().copied(), Some(SlotId(0)));
    guest.send(&ToHub::JoinMatch {
        id: state.id,
        role: bota_proto::Role::Player { seats: 1 },
    });
    let seen = guest.match_state();
    assert_eq!(seen.yours.first().copied(), Some(SlotId(1)));
    (host, guest, state.id)
}

/// Two accounts in one match the host has started, both on its game server
/// with their heroes picked and the match begun.
pub fn started(addr: SocketAddr, mode: ModeId) -> (TestClient, TestClient, MatchId) {
    let (mut host, mut guest, id) = gathered(addr, mode);
    host.send(&ToHub::StartMatch);
    host.pick(HeroId(0));
    guest.pick(HeroId(0));
    host.match_start();
    guest.match_start();
    (host, guest, id)
}
