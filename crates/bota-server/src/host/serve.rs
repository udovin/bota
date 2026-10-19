//! The game server's thread: its link to the hub, the players who show a
//! ticket, and the matches they are let into.

use std::io;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use bota_proto::{
    Assign, FromGame, GameReason, MatchId, MatchSetup, Nonce, Pass, Pick, PlayerId, Report, SlotId,
    Ticket, ToGame,
};

use crate::game::{MatchConfig, key_from_seed};
use crate::keys::{Prover, fresh, prove, proves, same};
use crate::net::{Connection, NetEvent, accept_loop};
use crate::replay::ReplayWriter;
use crate::run::{MatchEnded, MatchEvent, Participant, RunSetup, run_match};

/// How long a connection showing a ticket no match knows yet is waited for
/// before it is turned away, unless told otherwise.
pub const TICKET_GRACE: Duration = Duration::from_secs(5);

/// How a game server finds its hub and offers itself.
#[derive(Clone, Debug)]
pub struct HostOpts {
    /// Where the hub takes its game servers, as `host:port`.
    pub hub: String,
    /// The key the hub and its game servers share.
    pub key: [u8; 32],
    /// Where players connect to this server, as `host:port`.
    pub public: String,
    /// Matches it runs at once.
    pub capacity: u16,
    /// How long a ticket no match knows yet is waited for.
    pub ticket_grace: Duration,
}

/// Everything that reaches the game server's thread.
pub enum HostEvent {
    /// From a player's socket.
    Client(NetEvent<ToGame>),
    /// From the hub.
    Hub(NetEvent<Assign>),
    /// Every seat of a match has its hero, and the match is played.
    Picked(MatchId, Vec<Pick>),
    /// A match ended.
    Ended(MatchEnded),
    /// The next piece of a running match's replay.
    Replay(MatchId, Vec<u8>),
}

impl From<NetEvent<ToGame>> for HostEvent {
    fn from(event: NetEvent<ToGame>) -> HostEvent {
        HostEvent::Client(event)
    }
}

impl From<NetEvent<Assign>> for HostEvent {
    fn from(event: NetEvent<Assign>) -> HostEvent {
        HostEvent::Hub(event)
    }
}

/// Runs a game server on an already-bound listener for players: offers
/// itself to the hub and serves the matches it is given. Returns once the
/// hub's link ends, with the reason when it ended badly.
pub fn host(listener: TcpListener, opts: HostOpts) -> io::Result<()> {
    let stream = TcpStream::connect(&opts.hub)?;
    let (tx, rx) = channel();
    let accept_tx = tx.clone();
    thread::spawn(move || accept_loop::<ToGame, HostEvent>(listener, accept_tx));
    let hub = Connection::spawn::<Assign, HostEvent>(PlayerId(0), stream, tx.clone());
    let mut host = Host {
        opts,
        rx,
        tx,
        hub,
        asked: None,
        open: false,
        clients: Vec::new(),
        matches: Vec::new(),
        next_player: 1,
    };
    host.run()
}

/// One match this server runs.
struct Hosted {
    /// Which match.
    id: MatchId,
    /// Its thread's inbox.
    run: Sender<MatchEvent>,
    /// Everybody the hub lets into it.
    passes: Vec<Pass>,
}

/// One player connection.
struct Client {
    /// The socket.
    conn: Connection,
    /// The match it joined and the ticket it was let in with. Absent until
    /// its ticket is let in.
    joined: Option<(MatchId, Ticket)>,
    /// A ticket shown that no match knew yet, and until when it is waited
    /// for.
    waiting: Option<(Ticket, Instant)>,
}

struct Host {
    opts: HostOpts,
    rx: Receiver<HostEvent>,
    tx: Sender<HostEvent>,
    hub: Connection,
    /// The number this server asked the hub to prove itself over. Absent
    /// until the hub's challenge is answered.
    asked: Option<Nonce>,
    /// Whether the hub has proved the shared key.
    open: bool,
    clients: Vec<Client>,
    matches: Vec<Hosted>,
    next_player: u32,
}

/// What ends a game server: the link to its hub is gone or cannot be
/// trusted.
fn link_failed(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionAborted, why.to_string())
}

impl Host {
    fn run(&mut self) -> io::Result<()> {
        loop {
            let outcome = match self.rx.recv_timeout(Duration::from_millis(250)) {
                Ok(HostEvent::Hub(NetEvent::Msg(_, msg))) => self.on_hub(msg),
                Ok(HostEvent::Hub(NetEvent::Disconnected(_))) => {
                    Err(link_failed("the hub hung up"))
                }
                Ok(HostEvent::Hub(NetEvent::Connected(_))) => Ok(()),
                Ok(HostEvent::Client(event)) => {
                    self.on_client(event);
                    Ok(())
                }
                Ok(HostEvent::Picked(match_id, picks)) => {
                    self.hub.send(&Report::Picked { match_id, picks });
                    Ok(())
                }
                Ok(HostEvent::Ended(ended)) => {
                    self.on_ended(ended);
                    Ok(())
                }
                Ok(HostEvent::Replay(match_id, bytes)) => {
                    self.hub.send(&Report::Replay { match_id, bytes });
                    Ok(())
                }
                Err(RecvTimeoutError::Timeout) => Ok(()),
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            };
            if let Err(err) = outcome {
                for hosted in &self.matches {
                    let _ = hosted.run.send(MatchEvent::Stop);
                }
                self.hub.close();
                return Err(err);
            }
            self.turn_away_overdue();
        }
    }

    // ----- the hub -----

    fn on_hub(&mut self, msg: Assign) -> io::Result<()> {
        match msg {
            Assign::Challenge { nonce } => {
                let asked = Nonce(fresh());
                self.asked = Some(asked);
                self.hub.send(&Report::Register {
                    proof: prove(&self.opts.key, Prover::Game, &nonce),
                    nonce: asked,
                    addr: self.opts.public.clone(),
                    capacity: self.opts.capacity,
                });
            }
            Assign::Accepted { proof } => {
                let held = self
                    .asked
                    .is_some_and(|asked| proves(&self.opts.key, Prover::Hub, &asked, &proof));
                if !held {
                    return Err(link_failed("the hub did not prove the shared key"));
                }
                self.open = true;
            }
            // Nothing but the handshake is taken from a hub that has not
            // proved itself.
            _ if !self.open => {
                return Err(link_failed("the hub spoke before proving the shared key"));
            }
            Assign::Start { setup } => self.start(setup),
            Assign::Admit { match_id, pass } => {
                if let Some(hosted) = self.matches.iter_mut().find(|m| m.id == match_id) {
                    hosted.passes.push(pass);
                }
                self.let_waiting_in();
            }
            Assign::Abandon {
                match_id,
                seats,
                login,
            } => self.abandon(match_id, seats, login),
            Assign::Stop { match_id } => {
                if let Some(hosted) = self.matches.iter().find(|m| m.id == match_id) {
                    let _ = hosted.run.send(MatchEvent::Stop);
                }
            }
        }
        Ok(())
    }

    /// Starts a match on its own thread, its picks, its replay and its end
    /// reported back through this one.
    fn start(&mut self, setup: MatchSetup) {
        let id = setup.match_id;
        let cfg = MatchConfig {
            match_id: id.0,
            master_key: key_from_seed(setup.seed),
            picks: Vec::new(),
            map: setup.map,
            tick_rate: setup.tick_rate,
            mode: setup.tick,
            ack_timeout_ticks: setup.ack_timeout_ticks,
            rules: setup.rules,
        };
        let (run_tx, run_rx) = channel();
        let pieces = self.tx.clone();
        let picked = self.tx.clone();
        let ended = self.tx.clone();
        let run = RunSetup {
            id,
            cfg,
            seats: setup.seats,
            pick_time: Duration::from_millis(u64::from(setup.pick_ms)),
            events: run_rx,
            picked: Box::new(move |picks| {
                let _ = picked.send(HostEvent::Picked(id, picks));
            }),
            ended: Box::new(move |outcome| {
                let _ = ended.send(HostEvent::Ended(outcome));
            }),
            replay: ReplayWriter::to(Box::new(move |bytes| {
                let _ = pieces.send(HostEvent::Replay(id, bytes));
            })),
            abandon_after: Duration::from_millis(u64::from(setup.abandon_after_ms)),
            spectator_chat: setup.spectator_chat,
        };
        thread::spawn(move || run_match(run));
        self.matches.push(Hosted {
            id,
            run: run_tx,
            passes: setup.passes,
        });
        self.let_waiting_in();
    }

    /// Takes abandoned seats from everybody: the passes that drive them go,
    /// a connection let in on one is let go, and the match is told.
    fn abandon(&mut self, match_id: MatchId, seats: Vec<SlotId>, login: String) {
        let Some(hosted) = self.matches.iter_mut().find(|m| m.id == match_id) else {
            return;
        };
        let (revoked, kept): (Vec<Pass>, Vec<Pass>) = std::mem::take(&mut hosted.passes)
            .into_iter()
            .partition(|pass| pass.seats.iter().any(|seat| seats.contains(seat)));
        hosted.passes = kept;
        let _ = hosted.run.send(MatchEvent::Abandon { seats, login });
        while let Some(at) = self.clients.iter().position(|client| {
            client.joined.is_some_and(|(id, shown)| {
                id == match_id && revoked.iter().any(|pass| same(&pass.ticket.0, &shown.0))
            })
        }) {
            self.let_go(at);
        }
    }

    /// Tells the hub a match ended, and lets go of everybody in it once what
    /// they were last sent has gone out.
    fn on_ended(&mut self, ended: MatchEnded) {
        self.hub.send(&Report::Ended {
            match_id: ended.id,
            winner: ended.winner,
            stats: ended.stats,
            duration: ended.duration,
        });
        self.matches.retain(|m| m.id != ended.id);
        let in_it = |client: &Client| client.joined.is_some_and(|(id, _)| id == ended.id);
        for client in self.clients.iter().filter(|client| in_it(client)) {
            client.conn.close();
        }
        self.clients.retain(|client| !in_it(client));
    }

    // ----- players -----

    fn on_client(&mut self, event: NetEvent<ToGame>) {
        match event {
            NetEvent::Connected(stream) => {
                let id = PlayerId(self.next_player);
                self.next_player += 1;
                let conn = Connection::spawn::<ToGame, HostEvent>(id, stream, self.tx.clone());
                self.clients.push(Client {
                    conn,
                    joined: None,
                    waiting: None,
                });
            }
            NetEvent::Disconnected(player) => {
                let Some(at) = self.clients.iter().position(|c| c.conn.id == player) else {
                    return;
                };
                let gone = self.clients.remove(at);
                gone.conn.close();
                if let Some(hosted) = gone
                    .joined
                    .and_then(|(id, _)| self.matches.iter().find(|m| m.id == id))
                {
                    let _ = hosted.run.send(MatchEvent::Leave(player));
                }
            }
            NetEvent::Msg(player, msg) => self.on_client_msg(player, msg),
        }
    }

    fn on_client_msg(&mut self, player: PlayerId, msg: ToGame) {
        let Some(at) = self.clients.iter().position(|c| c.conn.id == player) else {
            return;
        };
        let Some((joined, _)) = self.clients[at].joined else {
            match msg {
                ToGame::Join { ticket } if self.clients[at].waiting.is_none() => {
                    let until = Instant::now() + self.opts.ticket_grace;
                    self.clients[at].waiting = Some((ticket, until));
                    self.let_waiting_in();
                }
                _ => self.turn_away(at, GameReason::NotJoined),
            }
            return;
        };
        let event = match msg {
            ToGame::Join { .. } => return self.turn_away(at, GameReason::NotJoined),
            ToGame::Select { seat, hero } => MatchEvent::Select { player, seat, hero },
            ToGame::Pick { seat, hero } => MatchEvent::Pick { player, seat, hero },
            ToGame::Orders { orders } => MatchEvent::Orders { player, orders },
            ToGame::Ack { tick } => MatchEvent::Ack { player, tick },
            ToGame::ViewAs { seat } => MatchEvent::ViewAs { player, seat },
            ToGame::Chat { text, team } => MatchEvent::Chat { player, text, team },
        };
        if let Some(hosted) = self.matches.iter().find(|m| m.id == joined) {
            let _ = hosted.run.send(event);
        }
    }

    /// Lets every connection whose ticket a match now knows into it. A
    /// ticket lets one connection in at a time: whoever came in with it
    /// before is let go.
    fn let_waiting_in(&mut self) {
        let mut at = 0;
        while at < self.clients.len() {
            let Some((ticket, _)) = self.clients[at].waiting else {
                at += 1;
                continue;
            };
            let found = self.matches.iter().find_map(|hosted| {
                hosted
                    .passes
                    .iter()
                    .find(|pass| same(&pass.ticket.0, &ticket.0))
                    .map(|pass| (hosted.id, pass.clone()))
            });
            let Some((id, pass)) = found else {
                at += 1;
                continue;
            };
            let before = self.clients.iter().position(|client| {
                client
                    .joined
                    .is_some_and(|(_, shown)| same(&shown.0, &ticket.0))
            });
            if let Some(before) = before {
                self.let_go(before);
                if before < at {
                    at -= 1;
                }
            }
            let client = &mut self.clients[at];
            client.waiting = None;
            client.joined = Some((id, ticket));
            let participant = Participant {
                player: client.conn.id,
                outbox: Arc::clone(&client.conn.outbox),
                login: pass.login,
                seats: pass.seats,
                eyes: None,
            };
            if let Some(hosted) = self.matches.iter().find(|m| m.id == id) {
                let _ = hosted.run.send(MatchEvent::Join(participant));
            }
            at += 1;
        }
    }

    /// Turns away every connection whose ticket has been waited for long
    /// enough.
    fn turn_away_overdue(&mut self) {
        let now = Instant::now();
        while let Some(at) = self
            .clients
            .iter()
            .position(|c| c.waiting.is_some_and(|(_, until)| until <= now))
        {
            self.turn_away(at, GameReason::UnknownTicket);
        }
    }

    /// Tells a connection why it is turned away, and lets it go.
    fn turn_away(&mut self, at: usize, reason: GameReason) {
        self.clients[at].conn.send(&FromGame::Refused { reason });
        self.let_go(at);
    }

    /// Closes a connection once what it was sent has gone out, and takes it
    /// out of its match if it was in one.
    fn let_go(&mut self, at: usize) {
        let gone = self.clients.remove(at);
        gone.conn.close();
        if let Some(hosted) = gone
            .joined
            .and_then(|(id, _)| self.matches.iter().find(|m| m.id == id))
        {
            let _ = hosted.run.send(MatchEvent::Leave(gone.conn.id));
        }
    }
}
