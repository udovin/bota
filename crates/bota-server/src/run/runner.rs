//! One match on its game server: the picking of heroes, then the tick loop
//! in either tick mode.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use bota_proto::{
    EntityId, FromGame, HeroId, MAX_CHAT_CHARS, MAX_ORDERS_PER_MESSAGE, MatchId, MatchInfo,
    MatchStats, Order, Pick, PickReason, PickSeat, PickState, PlayerId, RejectReason, ReplayRecord,
    SeatSetup, SlotId, Team, TickMode, UnitOrder, encode_frame_to_vec,
};

use crate::commands::{Deed, read_command};
use crate::game::{Cheat, Command, EventVisibility, HEROES, MatchConfig, World, hero_def};
use crate::keys::fresh_u64;
use crate::replay::ReplayWriter;
use crate::run::{MatchEvent, Participant, Whom};

/// The least time a match waits for its first seat holder to arrive before
/// it may be abandoned.
pub const FIRST_ARRIVAL: Duration = Duration::from_secs(10);

/// What a match tells its game server when it ends.
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
pub struct RunSetup {
    /// Which match.
    pub id: MatchId,
    /// Its terms. Its picks are filled in once its seats have made them.
    pub cfg: MatchConfig,
    /// Every seat, sorted by slot.
    pub seats: Vec<SeatSetup>,
    /// How long the seats have to pick their heroes.
    pub pick_time: Duration,
    /// What its game server will tell it.
    pub events: Receiver<MatchEvent>,
    /// Called once every seat has its hero, with every seat's pick in slot
    /// order. Never called for a match that ends first.
    pub picked: Box<dyn FnOnce(Vec<Pick>) + Send>,
    /// Called once, with how it ended.
    pub ended: Box<dyn FnOnce(MatchEnded) + Send>,
    /// Where its replay goes, from the moment it begins.
    pub replay: ReplayWriter,
    /// How long every seat may be away before the match is abandoned.
    pub abandon_after: Duration,
    /// Whether spectators may write in its chat.
    pub spectator_chat: bool,
}

/// The thread body: has the heroes picked, runs the match to its end, and
/// reports both.
pub fn run_match(setup: RunSetup) {
    let RunSetup {
        id,
        mut cfg,
        seats,
        pick_time,
        events,
        picked,
        ended,
        replay,
        abandon_after,
        spectator_chat,
    } = setup;
    let cheats = cfg.rules.cheats;
    let mut room = Room::new(id, seats, events, abandon_after, spectator_chat, cheats);
    let outcome = match room.pick_heroes(pick_time) {
        None => MatchEnded {
            id,
            winner: None,
            stats: MatchStats {
                duration: 0,
                slots: Vec::new(),
            },
            duration: 0,
        },
        Some(picks) => {
            picked(picks.clone());
            cfg.picks = picks;
            room.replay = replay;
            Runner::new(room, cfg).run()
        }
    };
    ended(outcome);
}

/// One accepted order, waiting for its tick.
#[derive(Clone, Copy)]
struct Waiting {
    /// The seat that drives the unit.
    slot: SlotId,
    /// The unit it is for.
    unit: EntityId,
    /// What to do.
    order: Order,
}

impl Waiting {
    /// The order as it is told to a watcher and kept in the replay.
    fn told(&self) -> UnitOrder {
        UnitOrder {
            unit: self.unit,
            order: self.order,
        }
    }
}

/// One seat's hero while heroes are picked.
#[derive(Clone, Copy, Default)]
struct Choice {
    /// The hero it picked, for good. Absent until it picks.
    picked: Option<HeroId>,
    /// The hero it is set on and has not picked. Absent while it is set on
    /// none, and once it has picked.
    selected: Option<HeroId>,
}

/// Who is in a match and what is said in it, from the first pick to the
/// end.
struct Room {
    id: MatchId,
    /// Every seat, sorted by slot.
    seats: Vec<SeatSetup>,
    participants: Vec<Participant>,
    events: Receiver<MatchEvent>,
    /// Where the replay goes. Records nothing while heroes are picked.
    replay: ReplayWriter,
    abandon_after: Duration,
    spectator_chat: bool,
    /// Whether cheats are taken.
    cheats_on: bool,
    /// Cheats waiting for the next tick, each with the seat it is for, in
    /// the order they were typed.
    cheats: Vec<(SlotId, Cheat)>,
    /// Since when no seat has been present, counted from the start until
    /// one arrives. Absent while one is.
    empty_since: Option<Instant>,
    /// Whether a seat holder has ever been present.
    arrived: bool,
    /// Seats given up for good, in the order they were.
    abandoned: Vec<SlotId>,
    stopped: bool,
}

impl Room {
    fn new(
        id: MatchId,
        seats: Vec<SeatSetup>,
        events: Receiver<MatchEvent>,
        abandon_after: Duration,
        spectator_chat: bool,
        cheats_on: bool,
    ) -> Room {
        Room {
            id,
            seats,
            participants: Vec::new(),
            events,
            replay: ReplayWriter::disabled(),
            abandon_after,
            spectator_chat,
            cheats_on,
            cheats: Vec::new(),
            empty_since: Some(Instant::now()),
            arrived: false,
            abandoned: Vec::new(),
            stopped: false,
        }
    }

    fn team_of(&self, slot: SlotId) -> Option<Team> {
        self.seats
            .iter()
            .find(|seat| seat.slot == slot)
            .map(|seat| seat.team)
    }

    /// The side a participant's seats are on. Absent for a spectator.
    fn side_of(&self, participant: &Participant) -> Option<Team> {
        participant
            .seats
            .first()
            .and_then(|slot| self.team_of(*slot))
    }

    /// Takes a connection in, in place of one it was in under before.
    fn admit(&mut self, participant: Participant) {
        self.participants.retain(|p| p.player != participant.player);
        self.participants.push(participant);
        self.note_presence();
    }

    fn leave(&mut self, player: PlayerId) {
        let Some(at) = self.participants.iter().position(|p| p.player == player) else {
            return;
        };
        let gone = self.participants.remove(at);
        if !gone.seats.is_empty() {
            self.broadcast(&FromGame::ParticipantLeft { slots: gone.seats });
        }
        self.note_presence();
    }

    fn note_presence(&mut self) {
        let seated = self.participants.iter().any(|p| !p.seats.is_empty());
        if seated {
            self.empty_since = None;
            self.arrived = true;
        } else if self.empty_since.is_none() {
            self.empty_since = Some(Instant::now());
        }
    }

    /// How long the match may go with no seat present: its abandon time,
    /// and no less than [`FIRST_ARRIVAL`] before anybody has come.
    fn allowed_absence(&self) -> Duration {
        if self.arrived {
            self.abandon_after
        } else {
            self.abandon_after.max(FIRST_ARRIVAL)
        }
    }

    /// When the match is abandoned unless a seat holder comes. Absent while
    /// one is present.
    fn deserted_at(&self) -> Option<Instant> {
        self.empty_since.map(|since| since + self.allowed_absence())
    }

    /// Whether no seat has been present for as long as the match may go
    /// without one.
    fn deserted(&self) -> bool {
        self.deserted_at().is_some_and(|at| at <= Instant::now())
    }

    fn broadcast(&self, msg: &FromGame) {
        if let Ok(frame) = encode_frame_to_vec(msg) {
            for p in &self.participants {
                p.outbox.push(frame.clone());
            }
        }
    }

    /// Sends one participant a message.
    fn send_to(&self, player: PlayerId, msg: &FromGame) {
        if let Some(p) = self.participants.iter().find(|p| p.player == player) {
            push(&p.outbox, msg);
        }
    }

    /// Tells one participant a line from the game server.
    fn tell(&self, player: PlayerId, text: &str) {
        self.send_to(
            player,
            &FromGame::Notice {
                text: text.to_string(),
            },
        );
    }

    /// Lends a spectator the eyes of a seat, or the whole map. Only a seat
    /// the match has has eyes to lend.
    fn view_as(&mut self, player: PlayerId, seat: Option<SlotId>) {
        let eyes = seat.filter(|seat| self.team_of(*seat).is_some());
        if let Some(p) = self
            .participants
            .iter_mut()
            .find(|p| p.player == player && p.seats.is_empty())
        {
            p.eyes = eyes;
        }
    }

    /// The seats a connection holds. Empty for a spectator and for one not
    /// in the match.
    fn seats_of(&self, player: PlayerId) -> Vec<SlotId> {
        self.participants
            .iter()
            .find(|p| p.player == player)
            .map(|p| p.seats.clone())
            .unwrap_or_default()
    }

    /// Lets every seat pick its hero. Picking ends once every seat not given
    /// up has picked, once a side has given up all of its seats, or when the
    /// time to pick runs out; every seat that has not picked is then given
    /// the hero it is set on, or one at random. Returns every seat's pick in
    /// slot order, or nothing when the match ended first: stopped, or with
    /// no seat present for too long.
    fn pick_heroes(&mut self, time: Duration) -> Option<Vec<Pick>> {
        let until = Instant::now() + time;
        let mut choices = vec![Choice::default(); self.seats.len()];
        loop {
            if self.stopped || self.deserted() {
                return None;
            }
            let all_picked = self.seats.iter().zip(&choices).all(|(seat, choice)| {
                choice.picked.is_some() || self.abandoned.contains(&seat.slot)
            });
            if all_picked || self.left_standing().is_some() || Instant::now() >= until {
                break;
            }
            let wake = self.deserted_at().map_or(until, |at| at.min(until));
            match self
                .events
                .recv_timeout(wake.saturating_duration_since(Instant::now()))
            {
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => self.stopped = true,
                Ok(event) => self.while_picking(event, &mut choices, until),
            }
        }
        let count = HEROES.len() as u64;
        let picks = self
            .seats
            .iter()
            .zip(choices)
            .map(|(seat, choice)| Pick {
                slot: seat.slot,
                team: seat.team,
                hero: choice
                    .picked
                    .or(choice.selected)
                    .unwrap_or_else(|| HeroId((fresh_u64() % count) as u16)),
            })
            .collect();
        Some(picks)
    }

    /// Answers one event while heroes are picked.
    fn while_picking(&mut self, event: MatchEvent, choices: &mut [Choice], until: Instant) {
        match event {
            MatchEvent::Join(participant) => {
                let player = participant.player;
                self.admit(participant);
                self.tell_picks(choices, until, Some(player));
            }
            MatchEvent::Leave(player) => self.leave(player),
            MatchEvent::Stop => self.stopped = true,
            MatchEvent::Select { player, seat, hero } => {
                match self.take_choice(player, seat, hero, false, choices) {
                    Ok(()) => self.tell_picks(choices, until, None),
                    Err(reason) => {
                        self.send_to(player, &FromGame::PickRefused { seat, hero, reason });
                    }
                }
            }
            MatchEvent::Pick { player, seat, hero } => {
                match self.take_choice(player, seat, hero, true, choices) {
                    Ok(()) => self.tell_picks(choices, until, None),
                    Err(reason) => {
                        self.send_to(player, &FromGame::PickRefused { seat, hero, reason });
                    }
                }
            }
            // Nothing stands to obey them yet.
            MatchEvent::Orders { player, orders } => {
                for order in orders {
                    self.send_to(
                        player,
                        &FromGame::OrderRejected {
                            order,
                            reason: RejectReason::NotPlaying,
                        },
                    );
                }
            }
            MatchEvent::Ack { .. } => {}
            MatchEvent::ViewAs { player, seat } => self.view_as(player, seat),
            MatchEvent::Chat { player, text, team } => self.chat(player, &text, team),
            MatchEvent::Abandon { seats, login } => self.abandon(seats, &login),
        }
    }

    /// Takes one seat's pick when `pick` is set, its selection otherwise, or
    /// says why it is not taken.
    fn take_choice(
        &self,
        player: PlayerId,
        seat: SlotId,
        hero: HeroId,
        pick: bool,
        choices: &mut [Choice],
    ) -> Result<(), PickReason> {
        let at = self
            .seats
            .iter()
            .position(|held| held.slot == seat)
            .filter(|_| self.seats_of(player).contains(&seat) && !self.abandoned.contains(&seat))
            .ok_or(PickReason::NotYourSeat)?;
        if choices[at].picked.is_some() {
            return Err(PickReason::AlreadyPicked);
        }
        if hero_def(hero).is_none() {
            return Err(PickReason::NoSuchHero);
        }
        choices[at] = if pick {
            Choice {
                picked: Some(hero),
                selected: None,
            }
        } else {
            Choice {
                picked: None,
                selected: Some(hero),
            }
        };
        Ok(())
    }

    /// Tells where the picking stands: to one connection, or to everybody.
    /// A seat's selection is told to its own side and to spectators.
    fn tell_picks(&self, choices: &[Choice], until: Instant, to: Option<PlayerId>) {
        let ms_left = u32::try_from(until.saturating_duration_since(Instant::now()).as_millis())
            .unwrap_or(u32::MAX);
        for p in self
            .participants
            .iter()
            .filter(|p| to.is_none_or(|player| p.player == player))
        {
            let side = self.side_of(p);
            let seats = self
                .seats
                .iter()
                .zip(choices)
                .map(|(seat, choice)| PickSeat {
                    slot: seat.slot,
                    team: seat.team,
                    login: seat.login.clone(),
                    hero: choice.picked,
                    selected: choice
                        .selected
                        .filter(|_| side.is_none_or(|side| side == seat.team)),
                })
                .collect();
            push(
                &p.outbox,
                &FromGame::Picking {
                    state: PickState {
                        seats,
                        yours: p.seats.clone(),
                        ms_left,
                    },
                },
            );
        }
    }

    /// Marks seats given up for good and tells everybody who gave them up.
    fn abandon(&mut self, seats: Vec<SlotId>, login: &str) {
        let newly: Vec<SlotId> = seats
            .into_iter()
            .filter(|slot| self.team_of(*slot).is_some() && !self.abandoned.contains(slot))
            .collect();
        if newly.is_empty() {
            return;
        }
        self.abandoned.extend(newly);
        let told = FromGame::Notice {
            text: format!("{login} abandoned the match"),
        };
        self.replay.record(&ReplayRecord::Msg(told.clone()));
        self.broadcast(&told);
    }

    /// The side left standing once every seat of the other is given up:
    /// [`Team::Neutral`] when both sides are. Absent while each side has a
    /// seat that is not.
    fn left_standing(&self) -> Option<Team> {
        let given_up = |team: Team| {
            let mut seats = self
                .seats
                .iter()
                .filter(|seat| seat.team == team)
                .peekable();
            seats.peek().is_some() && seats.all(|seat| self.abandoned.contains(&seat.slot))
        };
        match (given_up(Team::Radiant), given_up(Team::Dire)) {
            (true, true) => Some(Team::Neutral),
            (true, false) => Some(Team::Dire),
            (false, true) => Some(Team::Radiant),
            (false, false) => None,
        }
    }

    /// Answers a chat line: a command when it starts with a slash, said to
    /// the match otherwise. A blank line is nothing.
    fn chat(&mut self, player: PlayerId, text: &str, team: bool) {
        if text.chars().count() > MAX_CHAT_CHARS {
            self.tell(player, "that line is too long");
            return;
        }
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        match text.strip_prefix('/') {
            Some(line) => self.command(player, line.trim()),
            None => self.say(player, text, team),
        }
    }

    /// Who a participant speaks as and on which side, or why it may not
    /// speak here.
    fn voice(&self, player: PlayerId) -> Result<(String, Option<Team>), &'static str> {
        let speaker = self
            .participants
            .iter()
            .find(|p| p.player == player)
            .ok_or("not in this match")?;
        let side = self.side_of(speaker);
        if side.is_none() && !self.spectator_chat {
            return Err("spectators do not write in this match");
        }
        Ok((speaker.login.clone(), side))
    }

    /// Says a line to the match, or to one side of it and every spectator,
    /// and keeps it in the replay.
    fn say(&mut self, player: PlayerId, text: &str, team: bool) {
        let (from, side) = match self.voice(player) {
            Ok(voice) => voice,
            Err(why) => return self.tell(player, why),
        };
        let side_only = side.filter(|_| team);
        let said = FromGame::Chat {
            from,
            side,
            team: side_only.is_some(),
            text: text.to_string(),
        };
        if let Ok(frame) = encode_frame_to_vec(&said) {
            for p in &self.participants {
                let hears = match (side_only, self.side_of(p)) {
                    (Some(spoken), Some(theirs)) => spoken == theirs,
                    _ => true,
                };
                if hears {
                    p.outbox.push(frame.clone());
                }
            }
        }
        self.replay.record(&ReplayRecord::Msg(said));
    }

    /// Runs a command typed after a slash.
    fn command(&mut self, player: PlayerId, line: &str) {
        let deed = match read_command(line) {
            Ok(deed) => deed,
            Err(why) => return self.tell(player, &why),
        };
        match deed {
            Deed::Reply(lines) => {
                for text in lines {
                    self.tell(player, &text);
                }
            }
            Deed::Roll { low, high } => {
                let (login, _) = match self.voice(player) {
                    Ok(voice) => voice,
                    Err(why) => return self.tell(player, why),
                };
                let span = u64::from(high - low) + 1;
                let drawn = low + (fresh_u64() % span) as u32;
                let rolled = FromGame::Notice {
                    text: format!("{login} rolls {drawn} ({low}-{high})"),
                };
                self.broadcast(&rolled);
                self.replay.record(&ReplayRecord::Msg(rolled));
            }
            Deed::Cheat { cheat, whom } => {
                let Some(login) = self
                    .participants
                    .iter()
                    .find(|p| p.player == player && !p.seats.is_empty())
                    .map(|p| p.login.clone())
                else {
                    return self.tell(player, "spectators do not cheat");
                };
                self.take_cheat(player, &login, line, cheat, whom);
            }
        }
    }

    /// Keeps a cheat for the next tick, for every seat it names, and tells
    /// everybody it was typed. A match without cheats, or a cheat that names
    /// no seat, is answered to the one who typed it alone.
    fn take_cheat(&mut self, player: PlayerId, login: &str, line: &str, cheat: Cheat, whom: Whom) {
        let slots: Vec<SlotId> = match whom {
            Whom::Mine => self.seats_of(player),
            Whom::Seat(slot) => self.team_of(slot).map(|_| vec![slot]).unwrap_or_default(),
            Whom::Side(team) => self
                .seats
                .iter()
                .filter(|seat| seat.team == team)
                .map(|seat| seat.slot)
                .collect(),
            Whom::All => self.seats.iter().map(|seat| seat.slot).collect(),
        };
        if !self.cheats_on {
            return self.tell(player, "cheats are off in this match");
        }
        if slots.is_empty() {
            return self.tell(player, "no seat answers to that");
        }
        self.cheats
            .extend(slots.into_iter().map(|slot| (slot, cheat)));
        let told = FromGame::Notice {
            text: format!("{login}: /{line}"),
        };
        self.replay.record(&ReplayRecord::Msg(told.clone()));
        self.broadcast(&told);
    }
}

/// A match being played.
struct Runner {
    room: Room,
    cfg: MatchConfig,
    world: World,
    /// At most one order per unit; a later one replaces it.
    pending: Vec<Waiting>,
    /// The last tick each seat acknowledged, by slot.
    acked: Vec<u32>,
}

impl Runner {
    /// The match as it begins: its world made from its picks, and everybody
    /// in it told so.
    fn new(room: Room, cfg: MatchConfig) -> Runner {
        let seats = cfg.picks.len();
        let world = World::for_match(&cfg, cfg.rng());
        let runner = Runner {
            room,
            cfg,
            world,
            pending: Vec::new(),
            acked: vec![0; seats],
        };
        let info = runner.cfg.info();
        for participant in &runner.room.participants {
            runner.greet(participant, &info);
        }
        runner
    }

    /// Tells a connection the match and shows it the tick as it stands, so
    /// it has a view to act on before the next one.
    fn greet(&self, participant: &Participant, info: &MatchInfo) {
        push(
            &participant.outbox,
            &FromGame::MatchStart { info: info.clone() },
        );
        let view = match self.room.side_of(participant) {
            Some(team) => self.world.view(team),
            None => self.world.view_full(),
        };
        push(&participant.outbox, &FromGame::Snapshot { view });
        push(
            &participant.outbox,
            &FromGame::Events {
                tick: self.world.tick,
                events: Vec::new(),
            },
        );
    }

    /// Lets a connection in and greets it.
    fn join(&mut self, participant: Participant) {
        self.greet(&participant, &self.cfg.info());
        self.room.admit(participant);
    }

    fn run(&mut self) -> MatchEnded {
        self.room
            .replay
            .record(&ReplayRecord::Msg(FromGame::MatchStart {
                info: self.cfg.info(),
            }));
        let period = Duration::from_nanos(1_000_000_000 / u64::from(self.cfg.tick_rate.max(1)));
        let ack_timeout = period * self.cfg.ack_timeout_ticks.max(1);
        let started = Instant::now();
        let winner = loop {
            self.gather_input(started, period, ack_timeout);
            if self.room.stopped {
                break None;
            }
            if let Some(winner) = self.room.left_standing() {
                break Some(self.declare(winner));
            }
            if self.room.deserted() {
                break None;
            }
            // Orders apply in seat and unit order, whatever order they
            // arrived in, so the tick does not depend on the network.
            let mut waiting = std::mem::take(&mut self.pending);
            waiting.sort_by_key(|w| (w.slot, w.unit));
            let cmds: Vec<Command> = waiting
                .iter()
                .map(|w| Command {
                    slot: w.slot,
                    unit: Some(w.unit),
                    order: w.order,
                })
                .collect();
            self.room.replay.record(&ReplayRecord::Orders {
                tick: self.world.tick + 1,
                orders: waiting.iter().map(Waiting::told).collect(),
            });
            // Cheats go in seat order too, each seat's in the order typed.
            let mut cheats = std::mem::take(&mut self.room.cheats);
            cheats.sort_by_key(|(slot, _)| *slot);
            let mut events = Vec::new();
            for (slot, cheat) in cheats {
                self.world.cheat(slot, cheat, &mut events);
            }
            events.extend(self.world.advance(&cmds));
            self.send_tick(&events, &waiting);
            if let Some(winner) = self.world.victor() {
                break Some(self.declare(winner));
            }
        };
        let replay = std::mem::replace(&mut self.room.replay, ReplayWriter::disabled());
        replay.finish();
        MatchEnded {
            id: self.room.id,
            winner,
            stats: self.world.match_stats(),
            duration: self.world.tick,
        }
    }

    /// Tells everybody and the replay that the match is over, and who won.
    fn declare(&mut self, winner: Team) -> Team {
        let over = FromGame::MatchOver {
            winner,
            stats: self.world.match_stats(),
        };
        self.room.replay.record(&ReplayRecord::Msg(over.clone()));
        self.room.broadcast(&over);
        winner
    }

    /// Sends one tick to everybody, each through their own eyes, and records
    /// the fogless frames.
    fn send_tick(&mut self, events: &[crate::game::Event], taken: &[Waiting]) {
        let full = self.world.view_full();
        let full_frame = encode_frame_to_vec(&FromGame::Snapshot { view: full.clone() })
            .expect("a view always encodes");
        let team_frame = |team: Team| {
            encode_frame_to_vec(&FromGame::Snapshot {
                view: self.world.view(team),
            })
            .expect("a view always encodes")
        };
        let radiant_frame = team_frame(Team::Radiant);
        let dire_frame = team_frame(Team::Dire);
        self.room
            .replay
            .record(&ReplayRecord::Msg(FromGame::Snapshot { view: full }));

        for p in &self.room.participants {
            // The eyes a connection watches through: its own seats' side, or
            // the seat a spectator chose. Absent eyes see everything.
            let eyes = if p.seats.is_empty() { p.eyes } else { None };
            let viewing = self
                .room
                .side_of(p)
                .or_else(|| eyes.and_then(|slot| self.room.team_of(slot)));
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
                &FromGame::Events {
                    tick: self.world.tick,
                    events: visible,
                },
            );
            // A spectator watching through one seat's eyes is told that
            // seat's own orders. Watching everything is watching the game,
            // not the hands, and brings none; a seat knows its own.
            if let Some(eyes) = eyes {
                let told: Vec<UnitOrder> = taken
                    .iter()
                    .filter(|given| given.slot == eyes)
                    .map(Waiting::told)
                    .collect();
                if !told.is_empty() {
                    push(
                        &p.outbox,
                        &FromGame::Orders {
                            tick: self.world.tick,
                            orders: told,
                        },
                    );
                }
            }
        }
        if !events.is_empty() {
            self.room
                .replay
                .record(&ReplayRecord::Msg(FromGame::Events {
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
            if self.room.stopped {
                return;
            }
            if self.cfg.mode == TickMode::Lockstep && self.all_acked() {
                return;
            }
            // A match everybody has left waits no longer than it has left to
            // live, however long its tick would otherwise take.
            let until = self
                .room
                .deserted_at()
                .map_or(deadline, |at| deadline.min(at));
            let now = Instant::now();
            if now >= until {
                return;
            }
            match self.room.events.recv_timeout(until - now) {
                Err(RecvTimeoutError::Timeout) => return,
                Err(RecvTimeoutError::Disconnected) => {
                    self.room.stopped = true;
                    return;
                }
                Ok(MatchEvent::Join(participant)) => self.join(participant),
                Ok(MatchEvent::Leave(player)) => self.room.leave(player),
                Ok(MatchEvent::Stop) => self.room.stopped = true,
                Ok(
                    MatchEvent::Select { player, seat, hero }
                    | MatchEvent::Pick { player, seat, hero },
                ) => self.room.send_to(
                    player,
                    &FromGame::PickRefused {
                        seat,
                        hero,
                        reason: PickReason::NotPicking,
                    },
                ),
                Ok(MatchEvent::Orders { player, orders }) => self.take_orders(player, orders),
                Ok(MatchEvent::Ack { player, tick }) => {
                    for slot in self.room.seats_of(player) {
                        if let Some(acked) = self.acked.get_mut(usize::from(slot.0)) {
                            *acked = (*acked).max(tick);
                        }
                    }
                }
                Ok(MatchEvent::ViewAs { player, seat }) => self.room.view_as(player, seat),
                Ok(MatchEvent::Chat { player, text, team }) => self.room.chat(player, &text, team),
                Ok(MatchEvent::Abandon { seats, login }) => {
                    self.room.abandon(seats, &login);
                    if self.room.left_standing().is_some() {
                        return;
                    }
                }
            }
        }
    }

    /// Checks a connection's orders and keeps the ones that may be carried
    /// out, one per unit; the rest are given back refused.
    fn take_orders(&mut self, player: PlayerId, orders: Vec<UnitOrder>) {
        let seats = self.room.seats_of(player);
        let mut refused = Vec::new();
        for (at, given) in orders.into_iter().enumerate() {
            if at >= MAX_ORDERS_PER_MESSAGE {
                refused.push((given, RejectReason::TooManyOrders));
                continue;
            }
            let Some(slot) = self
                .world
                .seat_of_wire(given.unit)
                .filter(|slot| seats.contains(slot))
            else {
                refused.push((given, RejectReason::NotYourUnit));
                continue;
            };
            match self
                .world
                .validate_order(slot, Some(given.unit), &given.order)
            {
                Ok(()) => {
                    self.pending.retain(|w| w.unit != given.unit);
                    self.pending.push(Waiting {
                        slot,
                        unit: given.unit,
                        order: given.order,
                    });
                }
                Err(reason) => refused.push((given, reason)),
            }
        }
        for (order, reason) in refused {
            self.room
                .send_to(player, &FromGame::OrderRejected { order, reason });
        }
    }

    /// Whether every present seat has acknowledged the current tick.
    ///
    /// A match with nobody on a seat has nobody to hear from, and waits out
    /// the timeout instead.
    fn all_acked(&self) -> bool {
        let mut anybody = false;
        for slot in self.room.participants.iter().flat_map(|p| p.seats.iter()) {
            anybody = true;
            let acked = self.acked.get(usize::from(slot.0)).copied().unwrap_or(0);
            if acked < self.world.tick {
                return false;
            }
        }
        anybody
    }
}

fn push(outbox: &crate::net::Outbox, msg: &FromGame) {
    if let Ok(frame) = encode_frame_to_vec(msg) {
        outbox.push(frame);
    }
}
