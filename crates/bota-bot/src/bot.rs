//! The seam between a match and whatever decides what to do in it.
//!
//! A [`Bot`] holds one seat: it is handed the terms of the match, then one
//! tick at a time, and answers each tick with at most one [`Ask`]. A
//! [`Crew`] holds several seats on one connection and answers each tick for
//! all of them together; a [`Squad`] is a crew of one `Bot` to a seat.
//! [`play`] and [`play_crew`] are the loops that carry the conversation over
//! the sockets: to the hub for a seat, then to the game server it names.

use bota_proto::{
    EntityId, EventKind, FromGame, HeroId, MatchInfo, MatchStats, PlayerView, RejectReason, SlotId,
    Team, TickMode, UnitOrder, WorldView,
};

use crate::{Ask, Link, Seated, Want};

/// Anything that can hold a seat in a match.
///
/// The hero is chosen when the connection is made rather than asked for
/// here: heroes are picked on the game server before the match begins, before
/// there is a [`MatchInfo`] to decide from.
pub trait Bot {
    /// Told which seat the server gave this connection, before the match
    /// starts. Absent when no seat was free.
    fn seated(&mut self, slot: Option<SlotId>);

    /// Told the terms of the match, once, when it begins.
    fn match_started(&mut self, info: &MatchInfo);

    /// Handed one tick, and answers with at most one order.
    fn on_tick(&mut self, view: &WorldView) -> Option<Ask>;

    /// Told what happened during a tick, after that tick's snapshot.
    fn on_events(&mut self, _tick: u32, _events: &[EventKind]) {}

    /// Told that an order was not accepted: the order as it went out, and
    /// why.
    fn on_reject(&mut self, _refused: UnitOrder, _reason: RejectReason) {}

    /// Told how the match ended.
    fn finished(&mut self, _winner: Team, _stats: &MatchStats) {}
}

/// Anything that can hold several seats on one connection.
///
/// Handed each tick once for all its seats, and answers with what every unit
/// they drive should do.
pub trait Crew {
    /// Told which seats the server gave this connection, in slot order, all
    /// on one side, before the match starts.
    fn seated(&mut self, seats: &[SlotId]);

    /// Told the terms of the match, once, when it begins.
    fn match_started(&mut self, info: &MatchInfo);

    /// Handed one tick, and answers with any number of asks, each beside the
    /// seat whose hero an ask for no unit means. At most one order per unit
    /// survives a tick; the last one wins.
    fn on_tick(&mut self, view: &WorldView) -> Vec<(SlotId, Ask)>;

    /// Told what happened during a tick, after that tick's snapshot.
    fn on_events(&mut self, _tick: u32, _events: &[EventKind]) {}

    /// Told that an order given for one of its seats was not accepted: the
    /// order as it went out, and why.
    fn on_reject(&mut self, _seat: SlotId, _refused: UnitOrder, _reason: RejectReason) {}

    /// Told how the match ended.
    fn finished(&mut self, _winner: Team, _stats: &MatchStats) {}
}

/// One [`Bot`] to a seat, all on one connection.
///
/// The bots are made when the seats are known: the nth seat in slot order
/// gets `make(n, slot)`.
pub struct Squad<B> {
    make: Box<dyn FnMut(usize, SlotId) -> B>,
    bots: Vec<(SlotId, B)>,
}

impl<B: Bot> Squad<B> {
    /// A squad that makes its bots once it is seated.
    pub fn new(make: impl FnMut(usize, SlotId) -> B + 'static) -> Squad<B> {
        Squad {
            make: Box::new(make),
            bots: Vec::new(),
        }
    }

    /// Every seat and the bot in it, in slot order.
    pub fn bots(&self) -> &[(SlotId, B)] {
        &self.bots
    }
}

impl<B: Bot> Crew for Squad<B> {
    fn seated(&mut self, seats: &[SlotId]) {
        self.bots = seats
            .iter()
            .enumerate()
            .map(|(n, slot)| {
                let mut bot = (self.make)(n, *slot);
                bot.seated(Some(*slot));
                (*slot, bot)
            })
            .collect();
    }

    fn match_started(&mut self, info: &MatchInfo) {
        for (_, bot) in &mut self.bots {
            bot.match_started(info);
        }
    }

    fn on_tick(&mut self, view: &WorldView) -> Vec<(SlotId, Ask)> {
        self.bots
            .iter_mut()
            .filter_map(|(slot, bot)| bot.on_tick(view).map(|ask| (*slot, ask)))
            .collect()
    }

    fn on_events(&mut self, tick: u32, events: &[EventKind]) {
        for (_, bot) in &mut self.bots {
            bot.on_events(tick, events);
        }
    }

    fn on_reject(&mut self, seat: SlotId, refused: UnitOrder, reason: RejectReason) {
        if let Some((_, bot)) = self.bots.iter_mut().find(|(slot, _)| *slot == seat) {
            bot.on_reject(refused, reason);
        }
    }

    fn finished(&mut self, winner: Team, stats: &MatchStats) {
        for (_, bot) in &mut self.bots {
            bot.finished(winner, stats);
        }
    }
}

/// One borrowed [`Bot`] in the first seat a connection holds.
struct One<'a> {
    slot: Option<SlotId>,
    bot: &'a mut dyn Bot,
}

impl Crew for One<'_> {
    fn seated(&mut self, seats: &[SlotId]) {
        self.slot = seats.first().copied();
        self.bot.seated(self.slot);
    }

    fn match_started(&mut self, info: &MatchInfo) {
        self.bot.match_started(info);
    }

    fn on_tick(&mut self, view: &WorldView) -> Vec<(SlotId, Ask)> {
        match (self.slot, self.bot.on_tick(view)) {
            (Some(slot), Some(ask)) => vec![(slot, ask)],
            _ => Vec::new(),
        }
    }

    fn on_events(&mut self, tick: u32, events: &[EventKind]) {
        self.bot.on_events(tick, events);
    }

    fn on_reject(&mut self, _seat: SlotId, refused: UnitOrder, reason: RejectReason) {
        self.bot.on_reject(refused, reason);
    }

    fn finished(&mut self, winner: Team, stats: &MatchStats) {
        self.bot.finished(winner, stats);
    }
}

/// What to join, as what, and for how long.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chair {
    /// Where the hub listens.
    pub addr: String,
    /// The account to sign in as. Registered the first time it is used.
    pub login: String,
    /// Its password.
    pub password: String,
    /// Where to play.
    pub want: Want,
    /// Heroes to pick, one per seat in slot order, starting over when the
    /// list runs out.
    pub heroes: Vec<HeroId>,
    /// How many seats to ask for, all on one side, within the mode's
    /// limit for one account.
    pub seats: u8,
    /// Ticks to play before leaving. Absent plays until the match ends.
    pub limit: Option<u32>,
}

/// What one match came to for one seat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// The seat that was held. Absent when none was free.
    pub slot: Option<SlotId>,
    /// The side that seat played for.
    pub team: Option<Team>,
    /// Which side won. Absent when the bot left before the end.
    pub winner: Option<Team>,
    /// The last tick that was seen.
    pub ticks: u32,
    /// The seat's own last row of the scoreboard.
    pub mine: Option<PlayerView>,
    /// The final numbers, when the match ran to its end.
    pub stats: Option<MatchStats>,
    /// Orders the server would not take.
    pub rejected: u32,
    /// The reasons it gave, counted.
    pub refusals: Vec<(RejectReason, u32)>,
    /// Orders sent for the seat.
    pub ordered: u32,
}

/// Signs in, takes a seat where the chair says, and plays one match.
pub fn play(bot: &mut dyn Bot, chair: &Chair) -> std::io::Result<Outcome> {
    let mut link = sit_down(chair)?;
    let Some(seated) = link.take_seats()? else {
        return Err(std::io::Error::other("no seat was given"));
    };
    play_on(bot, &mut link, &seated, chair)
}

/// Signs in, takes seats where the chair says, and plays one match with a
/// crew. Returns what it came to for each seat, in slot order.
pub fn play_crew(crew: &mut dyn Crew, chair: &Chair) -> std::io::Result<Vec<Outcome>> {
    let mut link = sit_down(chair)?;
    let Some(seated) = link.take_seats()? else {
        return Err(std::io::Error::other("no seat was given"));
    };
    play_crew_on(crew, &mut link, &seated, chair)
}

/// Signs in and asks for what the chair wants, without waiting for a seat.
pub fn sit_down(chair: &Chair) -> std::io::Result<Link> {
    let mut link = Link::connect(&chair.addr)?;
    link.sign_in(&chair.login, &chair.password)?;
    link.ask_for(&chair.want, chair.seats)?;
    Ok(link)
}

/// Plays one match with one bot in the first of the seats given.
pub fn play_on(
    bot: &mut dyn Bot,
    link: &mut Link,
    seated: &Seated,
    chair: &Chair,
) -> std::io::Result<Outcome> {
    let first = seated
        .seats
        .first()
        .copied()
        .ok_or_else(|| std::io::Error::other("no seat was given"))?;
    let mut one = One { slot: None, bot };
    let alone = Seated {
        match_id: seated.match_id,
        seats: vec![first],
    };
    let mut outs = play_crew_on(&mut one, link, &alone, chair)?;
    Ok(outs.remove(0))
}

/// Plays one match, on a hub connection that has already been given its
/// seats, on the game server the hub names: picks the chair's heroes there
/// first. Returns what it came to for each seat, in slot order.
pub fn play_crew_on(
    crew: &mut dyn Crew,
    link: &mut Link,
    seated: &Seated,
    chair: &Chair,
) -> std::io::Result<Vec<Outcome>> {
    let mut outs: Vec<Outcome> = seated
        .seats
        .iter()
        .map(|slot| Outcome {
            slot: Some(*slot),
            ..Outcome::default()
        })
        .collect();
    let out_for = |outs: &mut Vec<Outcome>, seat: SlotId| -> Option<usize> {
        outs.iter().position(|out| out.slot == Some(seat))
    };
    crew.seated(&seated.seats);
    let Some(mut table) = link.wait_to_play(seated.match_id)? else {
        return Ok(outs);
    };
    let mut lockstep = false;
    // Which seat each unit was last ordered for, to tell a refusal whose it
    // is; a unit belongs to one seat for good.
    let mut seat_of: Vec<(EntityId, SlotId)> = Vec::new();
    // The seats a hero has been asked for.
    let mut asked: Vec<SlotId> = Vec::new();
    while let Some(msg) = table.hear()? {
        match msg {
            FromGame::Picking { state } => table.pick(&state, &chair.heroes, &mut asked)?,
            FromGame::MatchStart { info } => {
                lockstep = info.mode == TickMode::Lockstep;
                for out in &mut outs {
                    out.team = info
                        .picks
                        .iter()
                        .find(|pick| Some(pick.slot) == out.slot)
                        .map(|pick| pick.team);
                }
                crew.match_started(&info);
            }
            FromGame::Snapshot { view } => {
                for out in &mut outs {
                    out.ticks = view.tick;
                    out.mine = view
                        .players
                        .iter()
                        .find(|player| Some(player.slot) == out.slot)
                        .cloned();
                }
                let asks = crew.on_tick(&view);
                for (unit, seat) in table.orders(&asks, &view)? {
                    if let Some(at) = out_for(&mut outs, seat) {
                        outs[at].ordered += 1;
                    }
                    if !seat_of.contains(&(unit, seat)) {
                        seat_of.push((unit, seat));
                    }
                }
                if lockstep {
                    table.done_thinking(view.tick)?;
                }
                if chair.limit.is_some_and(|limit| view.tick >= limit) {
                    return Ok(outs);
                }
            }
            FromGame::Events { tick, events } => crew.on_events(tick, &events),
            FromGame::OrderRejected { order, reason } => {
                let Some(seat) = seat_of
                    .iter()
                    .find(|(unit, _)| *unit == order.unit)
                    .map(|(_, seat)| *seat)
                else {
                    continue;
                };
                if let Some(at) = out_for(&mut outs, seat) {
                    let out = &mut outs[at];
                    out.rejected += 1;
                    match out.refusals.iter_mut().find(|(had, _)| *had == reason) {
                        Some((_, many)) => *many += 1,
                        None => out.refusals.push((reason, 1)),
                    }
                }
                crew.on_reject(seat, order, reason);
            }
            FromGame::MatchOver { winner, stats } => {
                for out in &mut outs {
                    out.winner = Some(winner);
                    out.stats = Some(stats.clone());
                }
                crew.finished(winner, &stats);
                return Ok(outs);
            }
            FromGame::Refused { reason } => {
                return Err(std::io::Error::other(format!(
                    "the game server would not let this bot in: {reason:?}"
                )));
            }
            // A seat whose pick is turned down is given a hero at random.
            FromGame::PickRefused { .. }
            | FromGame::ParticipantLeft { .. }
            | FromGame::Orders { .. }
            | FromGame::Chat { .. }
            | FromGame::Notice { .. } => {}
        }
    }
    Ok(outs)
}

/// Plays every match the hub seats this account in, one after another.
///
/// A fresh crew is made for each. A match that ends leaves the account where
/// the hub brings it next; one left on the chair's limit is left on the hub
/// too. Returns what every match came to for each seat, match by match; ends
/// when no seat comes, which is when a tournament is over or the hub hangs
/// up.
pub fn play_every(
    mut make: impl FnMut() -> Box<dyn Crew>,
    chair: &Chair,
) -> std::io::Result<Vec<Outcome>> {
    let mut link = sit_down(chair)?;
    let mut played = Vec::new();
    while let Some(seated) = link.take_seats()? {
        let mut crew = make();
        let outs = play_crew_on(crew.as_mut(), &mut link, &seated, chair)?;
        let walked_off = !outs.is_empty()
            && outs.iter().all(|out| {
                out.winner.is_none() && chair.limit.is_some_and(|limit| out.ticks >= limit)
            });
        if walked_off {
            link.leave_match()?;
        }
        played.extend(outs);
    }
    Ok(played)
}
