//! The tick loop of one match, in either tick mode.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use bota_proto::{
    EntityId, MatchId, MatchStats, Order, PlayerId, ReplayRecord, ServerMsg, SlotId, SlotOrder,
    Team, TickMode, encode_frame_to_vec,
};

use crate::game::{Command, EventVisibility, MatchConfig, World};
use crate::replay::ReplayWriter;
use crate::run::{MatchEvent, Participant};

/// What a match tells the hub when it ends.
pub struct MatchEnded {
    /// Which match.
    pub id: MatchId,
    /// Which side won. Absent when the match was abandoned or stopped.
    pub winner: Option<Team>,
    /// The final numbers.
    pub stats: MatchStats,
    /// How many ticks were played.
    pub duration: u32,
}

/// Everything a match needs to run.
pub struct MatchSetup {
    /// Which match.
    pub id: MatchId,
    /// Its rules and picks.
    pub cfg: MatchConfig,
    /// Who is in it when it starts.
    pub participants: Vec<Participant>,
    /// What the hub will tell it.
    pub events: Receiver<MatchEvent>,
    /// Called once, with how it ended.
    pub ended: Box<dyn FnOnce(MatchEnded) + Send>,
    /// Where the replay is written while it runs. Absent records nothing.
    pub part: Option<PathBuf>,
    /// Where the replay is moved once it is whole.
    pub finished: Option<PathBuf>,
    /// How long every seat may be away before the match is abandoned.
    pub abandon_after: Duration,
}

/// The thread body: runs the match to its end and reports it.
pub fn run_match(setup: MatchSetup) {
    let ended = setup.ended;
    let mut runner = Runner::new(
        setup.id,
        setup.cfg,
        setup.participants,
        setup.events,
        setup.part,
        setup.finished,
        setup.abandon_after,
    );
    let outcome = runner.run();
    ended(outcome);
}

struct Runner {
    id: MatchId,
    cfg: MatchConfig,
    world: World,
    participants: Vec<Participant>,
    events: Receiver<MatchEvent>,
    replay: ReplayWriter,
    part: Option<PathBuf>,
    finished: Option<PathBuf>,
    abandon_after: Duration,
    /// Since when no seat has been present. Absent while one is.
    empty_since: Option<Instant>,
    pending: Vec<Option<(u32, Option<EntityId>, Order)>>,
    acked: Vec<u32>,
    stopped: bool,
}

impl Runner {
    fn new(
        id: MatchId,
        cfg: MatchConfig,
        participants: Vec<Participant>,
        events: Receiver<MatchEvent>,
        part: Option<PathBuf>,
        finished: Option<PathBuf>,
        abandon_after: Duration,
    ) -> Runner {
        let seats = cfg.picks.len();
        let replay = match &part {
            None => ReplayWriter::disabled(),
            Some(path) => ReplayWriter::create(path).unwrap_or_else(|_| ReplayWriter::disabled()),
        };
        let world = World::for_match(&cfg, cfg.rng());
        let mut runner = Runner {
            id,
            cfg,
            world,
            participants: Vec::new(),
            events,
            replay,
            part,
            finished,
            abandon_after,
            empty_since: None,
            pending: vec![None; seats],
            acked: vec![0; seats],
            stopped: false,
        };
        for participant in participants {
            runner.join(participant);
        }
        runner
    }

    fn team_of(&self, slot: SlotId) -> Option<Team> {
        self.cfg
            .picks
            .iter()
            .find(|pick| pick.slot == slot)
            .map(|pick| pick.team)
    }

    fn join(&mut self, participant: Participant) {
        self.participants.retain(|p| p.player != participant.player);
        participant.outbox.push(
            encode_frame_to_vec(&ServerMsg::MatchStart {
                info: self.cfg.info(),
            })
            .expect("a match info always encodes"),
        );
        self.participants.push(participant);
        self.note_presence();
    }

    fn leave(&mut self, player: PlayerId) {
        let Some(at) = self.participants.iter().position(|p| p.player == player) else {
            return;
        };
        let gone = self.participants.remove(at);
        if gone.seat.is_some() {
            let msg = ServerMsg::ParticipantLeft {
                player_id: gone.player,
                slot: gone.seat,
            };
            self.broadcast(&msg);
        }
        self.note_presence();
    }

    fn note_presence(&mut self) {
        let seated = self.participants.iter().any(|p| p.seat.is_some());
        if seated {
            self.empty_since = None;
        } else if self.empty_since.is_none() {
            self.empty_since = Some(Instant::now());
        }
    }

    fn broadcast(&self, msg: &ServerMsg) {
        if let Ok(frame) = encode_frame_to_vec(msg) {
            for p in &self.participants {
                p.outbox.push(frame.clone());
            }
        }
    }

    fn run(&mut self) -> MatchEnded {
        self.replay
            .record(&ReplayRecord::Msg(ServerMsg::MatchStart {
                info: self.cfg.info(),
            }));
        let period = Duration::from_nanos(1_000_000_000 / u64::from(self.cfg.tick_rate.max(1)));
        let ack_timeout = period * self.cfg.ack_timeout_ticks.max(1);
        let started = Instant::now();
        let winner = loop {
            self.gather_input(started, period, ack_timeout);
            if self.stopped {
                break None;
            }
            if self
                .empty_since
                .is_some_and(|since| since.elapsed() >= self.abandon_after)
            {
                break None;
            }
            let cmds: Vec<Command> = self
                .pending
                .iter()
                .enumerate()
                .filter_map(|(i, slot)| {
                    slot.map(|(_, unit, order)| Command {
                        slot: SlotId(i as u8),
                        unit,
                        order,
                    })
                })
                .collect();
            let taken: Vec<SlotOrder> = cmds
                .iter()
                .map(|c| SlotOrder {
                    slot: c.slot,
                    unit: c.unit,
                    order: c.order,
                })
                .collect();
            self.replay.record(&ReplayRecord::Orders {
                tick: self.world.tick + 1,
                orders: taken.clone(),
            });
            self.pending.fill(None);
            let events = self.world.advance(&cmds);
            self.send_tick(&events, &taken);
            if let Some(winner) = self.world.victor() {
                let over = ServerMsg::MatchOver {
                    winner,
                    stats: self.world.match_stats(),
                };
                self.replay.record(&ReplayRecord::Msg(over.clone()));
                self.broadcast(&over);
                break Some(winner);
            }
        };
        let replay = std::mem::replace(&mut self.replay, ReplayWriter::disabled());
        replay.finish();
        if let (Some(part), Some(finished)) = (&self.part, &self.finished) {
            let _ = std::fs::rename(part, finished);
        }
        MatchEnded {
            id: self.id,
            winner,
            stats: self.world.match_stats(),
            duration: self.world.tick,
        }
    }

    /// Sends one tick to everybody, each through their own eyes, and records
    /// the fogless frames.
    fn send_tick(&mut self, events: &[crate::game::Event], taken: &[SlotOrder]) {
        let full = self.world.view_full();
        let full_frame = encode_frame_to_vec(&ServerMsg::Snapshot { view: full.clone() })
            .expect("a view always encodes");
        let team_frame = |team: Team| {
            encode_frame_to_vec(&ServerMsg::Snapshot {
                view: self.world.view(team),
            })
            .expect("a view always encodes")
        };
        let radiant_frame = team_frame(Team::Radiant);
        let dire_frame = team_frame(Team::Dire);
        self.replay
            .record(&ReplayRecord::Msg(ServerMsg::Snapshot { view: full }));

        for p in &self.participants {
            // The eyes a connection watches through: its own seat's, or the
            // seat a spectator chose. Absent eyes see everything.
            let eyes = if p.seat.is_some() { None } else { p.eyes };
            let viewing = p
                .seat
                .and_then(|slot| self.team_of(slot))
                .or_else(|| eyes.and_then(|slot| self.team_of(slot)));
            let frame = match viewing {
                Some(Team::Radiant) => radiant_frame.clone(),
                Some(Team::Dire) => dire_frame.clone(),
                // Seats never sit on the jungle's side.
                Some(Team::Neutral) | None => full_frame.clone(),
            };
            p.outbox.push_snapshot(frame);
            let visible: Vec<bota_proto::EventKind> = events
                .iter()
                .filter(|e| match (e.visible_to, viewing) {
                    (_, None) => true,
                    (EventVisibility::Everyone, _) => true,
                    (EventVisibility::OneTeam(team), Some(mine)) => team == mine,
                })
                .map(|e| e.kind.clone())
                .collect();
            push(
                &p.outbox,
                &ServerMsg::Events {
                    tick: self.world.tick,
                    events: visible,
                },
            );
            // A spectator watching through one seat's eyes is told that
            // seat's own orders. Watching everything is watching the game,
            // not the hands, and brings none; a seat knows its own.
            if let Some(eyes) = eyes {
                let told: Vec<SlotOrder> = taken
                    .iter()
                    .filter(|given| given.slot == eyes)
                    .copied()
                    .collect();
                if !told.is_empty() {
                    push(
                        &p.outbox,
                        &ServerMsg::Orders {
                            tick: self.world.tick,
                            orders: told,
                        },
                    );
                }
            }
        }
        if !events.is_empty() {
            self.replay.record(&ReplayRecord::Msg(ServerMsg::Events {
                tick: self.world.tick,
                events: events.iter().map(|e| e.kind.clone()).collect(),
            }));
        }
    }

    /// Waits out the tick according to the mode, handling events meanwhile.
    fn gather_input(&mut self, started: Instant, period: Duration, ack_timeout: Duration) {
        let deadline = match self.cfg.mode {
            TickMode::Realtime => started + period * (self.world.tick + 1),
            TickMode::Lockstep => Instant::now() + ack_timeout,
        };
        loop {
            if self.stopped {
                return;
            }
            if self.cfg.mode == TickMode::Lockstep && self.all_acked() {
                return;
            }
            // A match everybody has left waits no longer than it has left to
            // live, however long its tick would otherwise take.
            let until = match self.empty_since {
                Some(since) => deadline.min(since + self.abandon_after),
                None => deadline,
            };
            let now = Instant::now();
            if now >= until {
                return;
            }
            match self.events.recv_timeout(until - now) {
                Err(RecvTimeoutError::Timeout) => return,
                Err(RecvTimeoutError::Disconnected) => {
                    self.stopped = true;
                    return;
                }
                Ok(MatchEvent::Join(participant)) => self.join(participant),
                Ok(MatchEvent::Leave(player)) => self.leave(player),
                Ok(MatchEvent::Stop) => self.stopped = true,
                Ok(MatchEvent::Order {
                    player,
                    seq,
                    unit,
                    order,
                }) => {
                    let Some(slot) = self.seat_of(player) else {
                        continue;
                    };
                    match self.world.validate_order(slot, unit, &order) {
                        Ok(()) => self.pending[usize::from(slot.0)] = Some((seq, unit, order)),
                        Err(reason) => {
                            if let Some(p) = self.participants.iter().find(|p| p.player == player) {
                                push(&p.outbox, &ServerMsg::OrderRejected { seq, reason });
                            }
                        }
                    }
                }
                Ok(MatchEvent::Ack { player, tick }) => {
                    if let Some(slot) = self.seat_of(player) {
                        let i = usize::from(slot.0);
                        self.acked[i] = self.acked[i].max(tick);
                    }
                }
                Ok(MatchEvent::ViewAs { player, seat }) => {
                    // Only a seat that is in the match has eyes to lend.
                    let eyes = seat.filter(|seat| self.team_of(*seat).is_some());
                    if let Some(p) = self
                        .participants
                        .iter_mut()
                        .find(|p| p.player == player && p.seat.is_none())
                    {
                        p.eyes = eyes;
                    }
                }
            }
        }
    }

    fn seat_of(&self, player: PlayerId) -> Option<SlotId> {
        self.participants
            .iter()
            .find(|p| p.player == player)
            .and_then(|p| p.seat)
    }

    /// Whether every present seat has acknowledged the current tick.
    ///
    /// A match with nobody on a seat has nobody to hear from, and waits out
    /// the timeout instead.
    fn all_acked(&self) -> bool {
        let mut anybody = false;
        for slot in self.participants.iter().filter_map(|p| p.seat) {
            anybody = true;
            if self.acked[usize::from(slot.0)] < self.world.tick {
                return false;
            }
        }
        anybody
    }
}

fn push(outbox: &crate::net::Outbox, msg: &ServerMsg) {
    if let Ok(frame) = encode_frame_to_vec(msg) {
        outbox.push(frame);
    }
}
