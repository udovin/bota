//! A written record of what a bot saw and what it did about it.
//!
//! One line a tick: where it stood, what it had, what was near, and the order
//! it gave. Enough to see why a match went the way it did without watching it,
//! and the shape a policy learned from recorded play would be trained on.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use bota_proto::{EventKind, MatchInfo, MatchStats, Order, SlotId, Team, UnitKind, WorldView};

use crate::{Ask, Bot, is_wave_creep, span};

/// A bot with everything it does written down as it does it.
pub struct Watched<'a, B> {
    /// The bot itself.
    bot: &'a mut B,
    /// Where the record goes.
    out: BufWriter<File>,
    /// Which seat is being followed.
    slot: Option<SlotId>,
}

impl<'a, B: Bot> Watched<'a, B> {
    /// Follows a bot, writing the record to a file.
    pub fn writing_to(bot: &'a mut B, path: &Path) -> std::io::Result<Watched<'a, B>> {
        let mut out = BufWriter::new(File::create(path)?);
        writeln!(out, "{TRAIL_HEADING}")?;
        Ok(Watched {
            bot,
            out,
            slot: None,
        })
    }
}

/// What the columns of a record hold.
pub const TRAIL_HEADING: &str =
    "# tick x y hp mana gold level last_hits denies deaths foes allies nearest order";

impl<B: Bot> Bot for Watched<'_, B> {
    fn seated(&mut self, slot: Option<SlotId>) {
        self.slot = slot;
        self.bot.seated(slot);
    }

    fn match_started(&mut self, info: &MatchInfo) {
        self.bot.match_started(info);
    }

    fn on_tick(&mut self, view: &WorldView) -> Option<Ask> {
        let ask = self.bot.on_tick(view);
        let _ = self.note(view, ask.as_ref());
        ask
    }

    fn on_events(&mut self, tick: u32, events: &[EventKind]) {
        self.bot.on_events(tick, events);
    }

    fn on_reject(&mut self, seq: u32, reason: bota_proto::RejectReason) {
        let _ = writeln!(self.out, "# rejected {seq}: {reason:?}");
        self.bot.on_reject(seq, reason);
    }

    fn finished(&mut self, winner: Team, stats: &MatchStats) {
        let _ = writeln!(self.out, "# {winner:?} won");
        let _ = self.out.flush();
        self.bot.finished(winner, stats);
    }
}

impl<B: Bot> Watched<'_, B> {
    /// Writes one line about one tick.
    fn note(&mut self, view: &WorldView, ask: Option<&Ask>) -> std::io::Result<()> {
        let Some(slot) = self.slot else {
            return Ok(());
        };
        let Some(seat) = view.players.iter().find(|player| player.slot == slot) else {
            return Ok(());
        };
        let Some(me) = seat
            .unit
            .and_then(|id| view.units.iter().find(|u| u.id == id))
        else {
            writeln!(
                self.out,
                "{} - - 0 0 {} {} {} {} {} - - - dead",
                view.tick,
                seat.gold.unwrap_or(0),
                seat.level,
                seat.last_hits,
                seat.denies,
                seat.deaths
            )?;
            return Ok(());
        };
        let near = |team_is_mine: bool| {
            view.units
                .iter()
                .filter(|unit| (unit.team == me.team) == team_is_mine && unit.id != me.id)
                .filter(|unit| unit.hp > 0 && span(me.pos, unit.pos) <= 1200.0)
                .count()
        };
        let nearest = view
            .units
            .iter()
            .filter(|unit| unit.team != me.team && unit.hp > 0)
            .filter(|unit| is_wave_creep(unit.kind) || unit.kind == UnitKind::Hero)
            .map(|unit| (span(me.pos, unit.pos), unit))
            .min_by(|one, other| {
                one.0
                    .partial_cmp(&other.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        writeln!(
            self.out,
            "{} {:.0} {:.0} {} {} {} {} {} {} {} {} {} {} {}",
            view.tick,
            me.pos.x.to_f32(),
            me.pos.y.to_f32(),
            me.hp,
            me.mana,
            seat.gold.unwrap_or(0),
            seat.level,
            seat.last_hits,
            seat.denies,
            seat.deaths,
            near(false),
            near(true),
            nearest.map_or(String::from("-"), |(far, unit)| format!(
                "{far:.0}/{}",
                unit.hp
            )),
            named(ask),
        )
    }
}

/// One word for what was ordered, whom it was for, and what it was aimed at.
fn named(ask: Option<&Ask>) -> String {
    let Some(ask) = ask else {
        return String::from("-");
    };
    let for_whom = match ask.unit {
        None => String::new(),
        Some(unit) => format!("@{}", unit.idx),
    };
    let what = match Some(&ask.order) {
        None => String::from("-"),
        Some(Order::Stop) => String::from("stop"),
        Some(Order::HoldPosition) => String::from("hold"),
        Some(Order::Move { pos }) => format!("move:{:.0},{:.0}", pos.x.to_f32(), pos.y.to_f32()),
        Some(Order::AttackMove { pos }) => {
            format!("push:{:.0},{:.0}", pos.x.to_f32(), pos.y.to_f32())
        }
        Some(Order::AttackUnit { target }) => format!("hit:{}", target.idx),
        Some(Order::CastAbility { slot, .. }) => format!("cast:{}", slot.0),
        Some(Order::UseItem { slot, .. }) => format!("use:{}", slot.0),
        Some(Order::MoveItem { from, to }) => format!("fetch:{}>{}", from.0, to.0),
        Some(Order::LevelUpAbility { slot }) => format!("level:{}", slot.0),
        Some(Order::BuyItem { item }) => format!("buy:{}", item.0),
        Some(Order::SellItem { slot }) => format!("sell:{}", slot.0),
    };
    format!("{what}{for_whom}")
}
