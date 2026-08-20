//! Playing one match: the loop that carries decisions to a server.
//!
//! What decides lives in [`Brain`](crate::Brain) and knows nothing about
//! sockets; what carries the decisions lives in [`Link`](crate::Link). This is
//! the seam between them, and the only place that knows the order the messages
//! come in.

use bota_proto::{
    EntityId, EventKind, HeroId, MatchInfo, MatchStats, PlayerView, ServerMsg, SlotId, Team,
    TickMode, UnitKind, WorldView,
};

use crate::{Ask, Brain, Link, Seated};

/// What a bot has to answer to be played.
pub trait Bot {
    /// Which seat it was given, once the server has handed one out. Absent
    /// means no seat was free.
    fn seated(&mut self, slot: Option<SlotId>);

    /// The terms of the match, once it begins.
    fn match_started(&mut self, info: &MatchInfo);

    /// One tick of the world, and at most one thing to do about it.
    ///
    /// One thing, because the server keeps one order per seat per tick and the
    /// last one wins. The answer names whom it is for: the seat's own hero
    /// unless it says otherwise.
    fn on_tick(&mut self, view: &WorldView) -> Option<Ask>;

    /// What happened during a tick that the snapshot does not show.
    fn on_events(&mut self, _tick: u32, _events: &[EventKind]) {}

    /// An order the server would not take.
    fn on_reject(&mut self, _seq: u32, _reason: bota_proto::RejectReason) {}

    /// How the match ended.
    fn finished(&mut self, _winner: Team, _stats: &MatchStats) {}
}

impl Bot for Brain {
    fn seated(&mut self, slot: Option<SlotId>) {
        self.slot = slot;
    }

    fn match_started(&mut self, info: &MatchInfo) {
        Brain::match_started(self, info);
    }

    fn on_tick(&mut self, view: &WorldView) -> Option<Ask> {
        self.decide(view)
    }

    fn on_events(&mut self, tick: u32, events: &[EventKind]) {
        self.heard(tick, events);
    }
}

/// What to join, as what, and for how long.
#[derive(Clone, Debug)]
pub struct Seat {
    /// Where the server listens.
    pub addr: String,
    /// The name the lobby shows.
    pub name: String,
    /// Which hero to ask for.
    pub hero: HeroId,
    /// Ticks to play before leaving. Absent plays until the match ends.
    pub limit: Option<u32>,
}

/// What one match came to for one bot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// Which seat it held.
    pub slot: Option<SlotId>,
    /// Which side that seat played for.
    pub team: Option<Team>,
    /// Which side won. Absent when the bot left before the end.
    pub winner: Option<Team>,
    /// The last tick it saw.
    pub ticks: u32,
    /// Its own last row of the scoreboard.
    pub mine: Option<PlayerView>,
    /// The final numbers, when the match ran to its end.
    pub stats: Option<MatchStats>,
    /// Orders the server would not take.
    pub rejected: u32,
    /// Damage it dealt to enemy heroes.
    ///
    /// Counted from the events rather than taken from [`MatchStats`]: the
    /// numbers arrive only when a match runs to its end, and a match played
    /// for a fixed span never does.
    pub hero_damage: i32,
    /// Damage it dealt to what the other side has built.
    pub structure_damage: i32,
    /// Damage it took from anything at all.
    pub damage_taken: i32,
}

/// Joins, plays, and returns what the match came to.
///
/// In lockstep every tick is acknowledged: the server advances no further
/// until each seat has, so a bot that stays quiet holds the whole match at the
/// ack timeout.
pub fn play<B: Bot + ?Sized>(bot: &mut B, seat: &Seat) -> std::io::Result<Outcome> {
    let (link, seated) = Link::join(&seat.addr, &seat.name, seat.hero)?;
    play_on(bot, link, seated, seat.limit)
}

/// The same, on a connection that has already been given its seat.
///
/// Which seat a connection is given follows the order the server sees them
/// arrive, so anything that cares which side it plays joins first and plays
/// after: joining is what settles it, and it has settled by the time this is
/// called.
pub fn play_on<B: Bot + ?Sized>(
    bot: &mut B,
    mut link: Link,
    seated: Seated,
    limit: Option<u32>,
) -> std::io::Result<Outcome> {
    let mut out = Outcome {
        slot: seated.slot,
        ..Outcome::default()
    };
    let lockstep = seated.mode == TickMode::Lockstep;
    bot.seated(seated.slot);
    // What each visible body is, for reading the damage events against. A body
    // that has fallen is kept: the blow that felled it names it.
    let mut what = Roll::new();
    while let Some(msg) = link.hear()? {
        match msg {
            // The seat was given out before this loop began; a second Welcome
            // would only say the same thing again.
            ServerMsg::Welcome { .. } => {}
            ServerMsg::MatchStart { info } => {
                out.team = out
                    .slot
                    .and_then(|slot| info.picks.iter().find(|pick| pick.slot == slot))
                    .map(|pick| pick.team);
                bot.match_started(&info);
            }
            ServerMsg::Snapshot { view } => {
                out.ticks = view.tick;
                if let Some(slot) = out.slot {
                    out.mine = view
                        .players
                        .iter()
                        .find(|player| player.slot == slot)
                        .cloned();
                }
                what.take_in(&view, out.mine.as_ref().and_then(|mine| mine.unit));
                if let Some(ask) = bot.on_tick(&view) {
                    link.order(ask)?;
                }
                if lockstep {
                    link.done_thinking(view.tick)?;
                }
                if limit.is_some_and(|limit| view.tick >= limit) {
                    return Ok(out);
                }
            }
            ServerMsg::Events { tick, events } => {
                what.count_the_damage(&events, &mut out);
                bot.on_events(tick, &events);
            }
            ServerMsg::OrderRejected { seq, reason } => {
                out.rejected += 1;
                bot.on_reject(seq, reason);
            }
            ServerMsg::MatchOver { winner, stats } => {
                out.winner = Some(winner);
                bot.finished(winner, &stats);
                out.stats = Some(stats);
                return Ok(out);
            }
            ServerMsg::LobbyState { .. } | ServerMsg::ParticipantLeft { .. } => {}
        }
    }
    Ok(out)
}

/// What every body seen so far is, kept so that a damage event can be read.
struct Roll {
    /// Every body and what side and kind it is, by handle.
    seen: Vec<(EntityId, Team, UnitKind)>,
    /// The body the bot drives.
    me: Option<EntityId>,
    /// The side it plays for.
    team: Option<Team>,
}

impl Roll {
    /// A roll with nobody on it.
    fn new() -> Roll {
        Roll {
            seen: Vec::new(),
            me: None,
            team: None,
        }
    }

    /// Adds whatever this snapshot shows that was not on it already.
    fn take_in(&mut self, view: &WorldView, me: Option<EntityId>) {
        self.me = me;
        for unit in &view.units {
            if Some(unit.id) == me {
                self.team = Some(unit.team);
            }
            match self.seen.binary_search_by_key(&unit.id, |(id, _, _)| *id) {
                Ok(_) => {}
                Err(at) => self.seen.insert(at, (unit.id, unit.team, unit.kind)),
            }
        }
    }

    /// What a body is, if it has ever been seen.
    fn what(&self, id: EntityId) -> Option<(Team, UnitKind)> {
        self.seen
            .binary_search_by_key(&id, |(id, _, _)| *id)
            .ok()
            .map(|at| (self.seen[at].1, self.seen[at].2))
    }

    /// Adds up what the bot dealt and what it took.
    fn count_the_damage(&self, events: &[EventKind], out: &mut Outcome) {
        for event in events {
            let EventKind::Damaged {
                source,
                target,
                amount,
                ..
            } = event
            else {
                continue;
            };
            if Some(*target) == self.me {
                out.damage_taken += amount;
                continue;
            }
            if *source != self.me {
                continue;
            }
            let Some((team, kind)) = self.what(*target) else {
                continue;
            };
            if Some(team) == self.team {
                continue;
            }
            match kind {
                UnitKind::Hero => out.hero_damage += amount,
                UnitKind::Tower | UnitKind::Ancient => out.structure_damage += amount,
                _ => {}
            }
        }
    }
}
