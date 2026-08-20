//! Watching the rule-driven bot, so the network can be shown what it did.
//!
//! A network started from nothing spends a very long time discovering that
//! walking into a tower is bad. It does not have to: there is already a bot
//! that plays a respectable lane, and every tick it decides something is a
//! tick the network could be told the answer to. So the first half of training
//! is not a search at all — it is copying.
//!
//! This wraps a bot and, whenever that bot gives an order, works out which of
//! the candidates the order was. What comes out is exactly the shape the
//! network learns from, and exactly the shape it will be asked to produce.

use bota_proto::{EventKind, MatchInfo, MatchStats, RejectReason, SlotId, Team, WorldView};

use crate::{Ask, Bot, Params, Recording, Upkeep, move_of, moves, row, state_of};

/// A bot with its choices written down as the network would see them.
pub struct Copying<'a, B> {
    /// The bot being copied.
    bot: &'a mut B,
    /// The numbers that draw up the candidates. They have to be the ones the
    /// copied bot plays by, or the list will not hold what it chose.
    params: Params,
    /// What was shown and what was taken.
    pub recording: Recording,
    /// Orders that were not among the candidates.
    pub missed: u32,
    /// Orders that were.
    pub matched: u32,
    /// Which seat is being watched.
    slot: Option<SlotId>,
    /// What is kept between ticks.
    keep: Upkeep,
}

impl<'a, B: Bot> Copying<'a, B> {
    /// Watches a bot that plays by these numbers.
    pub fn new(bot: &'a mut B, params: Params) -> Copying<'a, B> {
        Copying {
            bot,
            params,
            recording: Recording::new(),
            missed: 0,
            matched: 0,
            slot: None,
            keep: Upkeep::new(),
        }
    }

    /// What part of the orders given were candidates the network could have
    /// picked.
    ///
    /// Short of one means the candidate list is missing something the bot does,
    /// and the network cannot be taught to do it.
    pub fn covered(&self) -> f32 {
        let seen = self.matched + self.missed;
        if seen == 0 {
            return 1.0;
        }
        self.matched as f32 / seen as f32
    }
}

impl<B: Bot> Bot for Copying<'_, B> {
    fn seated(&mut self, slot: Option<SlotId>) {
        self.slot = slot;
        self.bot.seated(slot);
    }

    fn match_started(&mut self, info: &MatchInfo) {
        self.keep.match_started(info);
        self.bot.match_started(info);
    }

    fn on_tick(&mut self, view: &WorldView) -> Option<Ask> {
        let ask = self.bot.on_tick(view);
        // Only the ticks it decided something on are worth copying. On the
        // rest it was standing by what it had already said, and a frame
        // teaching the network to repeat itself teaches it nothing.
        let asked = ask?;
        let slot = self.slot?;
        let Some(sight) = self.keep.look(view, slot) else {
            return ask;
        };
        let wants = moves(&sight, slot, self.keep.trees(), &self.params);
        match wants.iter().position(|want| want.ask() == asked) {
            None => self.missed += 1,
            Some(chosen) => {
                self.matched += 1;
                let state = state_of(&sight, &self.params);
                let rows = wants
                    .iter()
                    .map(|want| row(&state, &move_of(&sight, want, &self.params)))
                    .collect();
                self.recording.put(rows, chosen);
            }
        }
        ask
    }

    fn on_events(&mut self, tick: u32, events: &[EventKind]) {
        self.keep.heard(tick, events, &self.params);
        self.bot.on_events(tick, events);
    }

    fn on_reject(&mut self, seq: u32, reason: RejectReason) {
        self.bot.on_reject(seq, reason);
    }

    fn finished(&mut self, winner: Team, stats: &MatchStats) {
        self.bot.finished(winner, stats);
    }
}
