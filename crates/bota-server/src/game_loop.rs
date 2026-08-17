//! The server: lobby phase, then the tick loop in either mode.

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use bota_proto::{
    ClientMsg, MapId, Order, PlayerId, ReplayRecord, Role, ServerMsg, SlotId, Team, TickMode,
    encode_frame_to_vec,
};

use crate::lobby::Roster;
use crate::net::{Connection, NetEvent, accept_loop};
use crate::replay::ReplayWriter;
use crate::sim::{Command, EventVisibility, MatchConfig, World};

/// Everything the command line decides.
#[derive(Clone, Debug)]
pub struct ServerOpts {
    /// How ticks advance.
    pub mode: TickMode,
    /// Ticks per wall-clock second.
    pub tick_rate: u16,
    /// Seats in the match.
    pub players: u8,
    /// Where to write the replay. Absent records nothing.
    pub replay: Option<PathBuf>,
    /// Seed of the match randomness and the match id.
    pub seed: u64,
    /// Lockstep: how many tick-lengths to wait for a straggler.
    pub ack_timeout_ticks: u32,
}

/// Runs one match on an already-bound listener and returns when it ends.
pub fn run(listener: TcpListener, opts: ServerOpts) -> std::io::Result<()> {
    let (tx, rx) = channel();
    let accept_tx = tx.clone();
    thread::spawn(move || accept_loop(listener, accept_tx));
    let mut server = Server {
        opts,
        rx,
        _tx: tx,
        conns: Vec::new(),
        spectators: Vec::new(),
        roster: Roster::new(0),
        next_player: 1,
    };
    server.roster = Roster::new(server.opts.players);
    let cfg = server.lobby_phase();
    server.match_phase(cfg);
    Ok(())
}

struct Server {
    opts: ServerOpts,
    rx: Receiver<NetEvent>,
    /// Keeps the channel open across reader-thread turnover.
    _tx: Sender<NetEvent>,
    conns: Vec<Connection>,
    /// Connections that said Hello as spectators.
    spectators: Vec<PlayerId>,
    roster: Roster,
    next_player: u32,
}

impl Server {
    fn conn(&self, id: PlayerId) -> Option<&Connection> {
        self.conns.iter().find(|c| c.id == id)
    }

    fn broadcast(&self, msg: &ServerMsg) {
        for conn in &self.conns {
            conn.send(msg);
        }
    }

    fn broadcast_lobby(&self) {
        self.broadcast(&ServerMsg::LobbyState {
            slots: self.roster.lobby_slots(),
        });
    }

    fn drop_conn(&mut self, id: PlayerId) {
        if let Some(conn) = self.conn(id) {
            conn.close();
        }
        self.conns.retain(|c| c.id != id);
        self.spectators.retain(|&p| p != id);
    }

    /// Accepts a fresh stream and hands out a `PlayerId`.
    fn adopt(&mut self, stream: std::net::TcpStream) -> PlayerId {
        let id = PlayerId(self.next_player);
        self.next_player += 1;
        let conn = Connection::spawn(id, stream, self._tx.clone());
        self.conns.push(conn);
        id
    }

    /// A Hello: seats players and bots, registers spectators.
    ///
    /// Returns whether the peer was accepted. `info` carries the running
    /// match to a spectator joining after the start.
    fn greet(
        &mut self,
        id: PlayerId,
        role: Role,
        name: String,
        info: Option<&MatchConfig>,
    ) -> bool {
        if self.roster.seat_of(id).is_some() || self.spectators.contains(&id) {
            return true; // a second Hello changes nothing
        }
        let (slot, accepted) = match role {
            Role::Spectator => {
                self.spectators.push(id);
                (None, true)
            }
            Role::Player | Role::Bot => {
                if info.is_some() {
                    (None, false) // seats are not handed out mid-match
                } else {
                    match self.roster.free_seat_mut() {
                        None => (None, false),
                        Some(seat) => {
                            seat.player = Some(id);
                            seat.name = name;
                            seat.role = Some(role);
                            seat.ready = false;
                            (Some(seat.slot), true)
                        }
                    }
                }
            }
        };
        if !accepted {
            self.drop_conn(id);
            return false;
        }
        if let Some(conn) = self.conn(id) {
            conn.send(&ServerMsg::Welcome {
                player_id: id,
                slot,
                tick_rate: self.opts.tick_rate,
                mode: self.opts.mode,
            });
            if let Some(cfg) = info {
                conn.send(&ServerMsg::MatchStart { info: cfg.info() });
            }
        }
        true
    }

    /// Collects seats and picks until everyone is ready.
    fn lobby_phase(&mut self) -> MatchConfig {
        loop {
            let event = match self.rx.recv() {
                Ok(event) => event,
                Err(_) => unreachable!("the server holds a sender"),
            };
            match event {
                NetEvent::Connected(stream) => {
                    self.adopt(stream);
                }
                NetEvent::Disconnected(id) => {
                    self.roster.release(id);
                    self.drop_conn(id);
                    self.broadcast_lobby();
                }
                NetEvent::Msg(id, msg) => match msg {
                    ClientMsg::Hello { role, name } => {
                        if self.greet(id, role, name, None) {
                            self.broadcast_lobby();
                        }
                    }
                    ClientMsg::PickHero { hero } => {
                        if let Some(seat) = self.roster.seat_of_mut(id) {
                            seat.hero = Some(hero);
                            self.broadcast_lobby();
                        }
                    }
                    ClientMsg::SetReady(ready) => {
                        if let Some(seat) = self.roster.seat_of_mut(id) {
                            seat.ready = ready;
                            self.broadcast_lobby();
                            if self.roster.all_ready() {
                                return self.build_config();
                            }
                        }
                    }
                    ClientMsg::Order { .. } | ClientMsg::Ack { .. } => {}
                },
            }
        }
    }

    fn build_config(&self) -> MatchConfig {
        let mut master_key = [0u8; 32];
        for (i, b) in self
            .opts
            .seed
            .to_le_bytes()
            .iter()
            .cycle()
            .take(32)
            .enumerate()
        {
            master_key[i] = *b;
        }
        MatchConfig {
            match_id: self.opts.seed,
            master_key,
            picks: self.roster.picks(),
            map: MapId(0),
            tick_rate: self.opts.tick_rate,
            mode: self.opts.mode,
            ack_timeout_ticks: self.opts.ack_timeout_ticks,
        }
    }

    /// Runs the match to its end.
    fn match_phase(&mut self, cfg: MatchConfig) {
        let mut world = World::new(&cfg, cfg.rng());
        let mut replay = match &self.opts.replay {
            None => ReplayWriter::disabled(),
            Some(path) => ReplayWriter::create(path).unwrap_or_else(|_| ReplayWriter::disabled()),
        };
        replay.record(&ReplayRecord::Msg(ServerMsg::MatchStart {
            info: cfg.info(),
        }));
        self.broadcast(&ServerMsg::MatchStart { info: cfg.info() });

        let period = Duration::from_nanos(1_000_000_000 / u64::from(self.opts.tick_rate.max(1)));
        let ack_timeout = period * self.opts.ack_timeout_ticks.max(1);
        let started = Instant::now();
        let mut pending: Vec<Option<(u32, Order)>> = vec![None; self.roster.seats.len()];
        let mut acked: Vec<u32> = vec![0; self.roster.seats.len()];

        loop {
            self.gather_input(
                &world,
                &cfg,
                &mut pending,
                &mut acked,
                started,
                period,
                ack_timeout,
            );
            if self.roster.seats.iter().all(|s| s.player.is_none()) {
                break; // nobody left to play or watch the seats
            }
            let cmds: Vec<Command> = pending
                .iter()
                .enumerate()
                .filter_map(|(i, slot)| {
                    slot.map(|(_, order)| Command {
                        slot: SlotId(i as u8),
                        order,
                    })
                })
                .collect();
            replay.record(&ReplayRecord::Orders {
                tick: world.tick + 1,
                orders: cmds.iter().map(|c| (c.slot, c.order)).collect(),
            });
            pending.fill(None);
            let events = world.step(&cmds);

            let full = world.view_full();
            let full_frame = encode_frame_to_vec(&ServerMsg::Snapshot { view: full.clone() })
                .expect("a view always encodes");
            let team_frame = |team: Team| {
                encode_frame_to_vec(&ServerMsg::Snapshot {
                    view: world.view(team),
                })
                .expect("a view always encodes")
            };
            let radiant_frame = team_frame(Team::Radiant);
            let dire_frame = team_frame(Team::Dire);
            replay.record(&ReplayRecord::Msg(ServerMsg::Snapshot { view: full }));

            for conn in &self.conns {
                let seat_team = self.roster.seat_of(conn.id).map(|s| s.team);
                let frame = match seat_team {
                    Some(Team::Radiant) => radiant_frame.clone(),
                    Some(Team::Dire) => dire_frame.clone(),
                    None => full_frame.clone(),
                };
                conn.send_snapshot(frame);
                let visible: Vec<bota_proto::EventKind> = events
                    .iter()
                    .filter(|e| match (e.visible_to, seat_team) {
                        (_, None) => true,
                        (EventVisibility::Everyone, _) => true,
                        (EventVisibility::OneTeam(team), Some(mine)) => team == mine,
                    })
                    .map(|e| e.kind.clone())
                    .collect();
                if !visible.is_empty() {
                    conn.send(&ServerMsg::Events {
                        tick: world.tick,
                        events: visible,
                    });
                }
            }
            if !events.is_empty() {
                replay.record(&ReplayRecord::Msg(ServerMsg::Events {
                    tick: world.tick,
                    events: events.iter().map(|e| e.kind.clone()).collect(),
                }));
            }

            if let Some(winner) = world.winner() {
                let over = ServerMsg::MatchOver {
                    winner,
                    stats: world.stats(),
                };
                replay.record(&ReplayRecord::Msg(over.clone()));
                self.broadcast(&over);
                break;
            }
        }
        replay.finish();
        for conn in &self.conns {
            conn.close();
        }
    }

    /// Waits out the tick according to the mode, handling messages meanwhile.
    #[expect(clippy::too_many_arguments, reason = "the tick loop's working set")]
    fn gather_input(
        &mut self,
        world: &World,
        cfg: &MatchConfig,
        pending: &mut [Option<(u32, Order)>],
        acked: &mut [u32],
        started: Instant,
        period: Duration,
        ack_timeout: Duration,
    ) {
        let deadline = match self.opts.mode {
            TickMode::Realtime => started + period * (world.tick + 1),
            TickMode::Lockstep => Instant::now() + ack_timeout,
        };
        loop {
            if self.opts.mode == TickMode::Lockstep && self.all_acked(world.tick, acked) {
                break;
            }
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match self.rx.recv_timeout(deadline - now) {
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => break,
                Ok(NetEvent::Connected(stream)) => {
                    self.adopt(stream);
                }
                Ok(NetEvent::Disconnected(id)) => {
                    let slot = self.roster.seat_of(id).map(|s| s.slot);
                    self.roster.release(id);
                    self.drop_conn(id);
                    self.broadcast(&ServerMsg::ParticipantLeft {
                        player_id: id,
                        slot,
                    });
                }
                Ok(NetEvent::Msg(id, msg)) => match msg {
                    ClientMsg::Hello { role, name } => {
                        self.greet(id, role, name, Some(cfg));
                    }
                    ClientMsg::Order { seq, order } => {
                        let Some(slot) = self.roster.seat_of(id).map(|s| s.slot) else {
                            continue;
                        };
                        match world.validate(slot, &order) {
                            Ok(()) => pending[usize::from(slot.0)] = Some((seq, order)),
                            Err(reason) => {
                                if let Some(conn) = self.conn(id) {
                                    conn.send(&ServerMsg::OrderRejected { seq, reason });
                                }
                            }
                        }
                    }
                    ClientMsg::Ack { tick } => {
                        if let Some(seat) = self.roster.seat_of(id) {
                            let i = usize::from(seat.slot.0);
                            acked[i] = acked[i].max(tick);
                        }
                    }
                    ClientMsg::PickHero { .. } | ClientMsg::SetReady(_) => {}
                },
            }
        }
    }

    /// Whether every connected seat has acknowledged this tick.
    fn all_acked(&self, tick: u32, acked: &[u32]) -> bool {
        self.roster
            .seats
            .iter()
            .filter(|s| s.player.is_some())
            .all(|s| acked[usize::from(s.slot.0)] >= tick)
    }
}
