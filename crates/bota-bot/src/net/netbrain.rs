//! A bot that decides with the network.
//!
//! Every tick it draws up what it could do, shows the network each of them
//! alongside the tick they would be done in, and takes the one scored highest.
//! Nothing here knows what a last hit is: which candidates exist is the rule
//! bot's vocabulary, and which of them is worth taking is the whole of what
//! was learned.
//!
//! It can be asked to choose loosely rather than take the best. That is what
//! training does: a policy that always takes what it already believes never
//! finds out whether it was right.

use bota_proto::{EventKind, MatchInfo, MatchStats, Order, RejectReason, SlotId, Team, WorldView};

use crate::{Ask, Bot, Dice, Net, Params, Recording, Upkeep, Want, move_of, moves, row, state_of};

/// One bot that plays by a network.
pub struct NetBrain {
    /// Which seat it drives.
    pub slot: Option<SlotId>,
    /// The numbers that draw up what it could do, and describe it.
    pub params: Params,
    /// What was learned.
    pub net: Net,
    /// How loosely it chooses. Zero always takes the best.
    pub heat: f32,
    /// Where its choices are written down, while somebody is watching.
    pub recording: Option<Recording>,
    /// What it keeps between ticks.
    keep: Upkeep,
    /// Where a loose choice is drawn from.
    dice: Dice,
}

impl NetBrain {
    /// A bot playing by these weights, taking the best it is shown.
    pub fn new(net: Net, params: Params) -> NetBrain {
        NetBrain {
            slot: None,
            params,
            net,
            heat: 0.0,
            recording: None,
            keep: Upkeep::new(),
            dice: Dice::from_seed(1),
        }
    }

    /// The same, choosing loosely, from a stream of its own.
    pub fn loosely(net: Net, params: Params, heat: f32, seed: u64) -> NetBrain {
        NetBrain {
            heat,
            dice: Dice::from_seed(seed),
            ..NetBrain::new(net, params)
        }
    }

    /// Starts writing down what it is shown and what it takes.
    pub fn watch(&mut self) {
        self.recording = Some(Recording::new());
    }

    /// What it decides this tick.
    pub fn decide(&mut self, view: &WorldView) -> Option<Ask> {
        let slot = self.slot?;
        let Some(sight) = self.keep.look(view, slot) else {
            self.keep.forget();
            return None;
        };
        let wants = moves(&sight, slot, self.keep.trees(), &self.params);
        if wants.is_empty() {
            return None;
        }
        let state = state_of(&sight, &self.params);
        let rows: Vec<Vec<f32>> = wants
            .iter()
            .map(|want| row(&state, &move_of(&sight, want, &self.params)))
            .collect();
        let scores = self.net.scores(&rows).ok()?;
        let at = self.pick(&scores);
        if let Some(recording) = self.recording.as_mut() {
            recording.put(rows, at);
        }
        self.keep.say(*wants.get(at)?, view.tick, &self.params)
    }

    /// Which of the scores to take: the best of them, or one drawn in
    /// proportion to how good the network thinks each is.
    fn pick(&mut self, scores: &[f32]) -> usize {
        if self.heat <= 0.0 {
            return best_of(scores);
        }
        let highest = scores.iter().copied().fold(f32::MIN, f32::max);
        let weights: Vec<f32> = scores
            .iter()
            .map(|score| ((score - highest) / self.heat).exp())
            .collect();
        let total: f32 = weights.iter().sum();
        if !total.is_finite() || total <= 0.0 {
            return best_of(scores);
        }
        let mut drawn = self.dice.unit() * total;
        for (at, weight) in weights.iter().enumerate() {
            drawn -= weight;
            if drawn <= 0.0 {
                return at;
            }
        }
        weights.len() - 1
    }
}

/// Whichever score is highest, and the first of equals.
pub fn best_of(scores: &[f32]) -> usize {
    scores
        .iter()
        .enumerate()
        .fold((0, f32::MIN), |(best, highest), (at, score)| {
            if *score > highest {
                (at, *score)
            } else {
                (best, highest)
            }
        })
        .0
}

impl Bot for NetBrain {
    fn seated(&mut self, slot: Option<SlotId>) {
        self.slot = slot;
    }

    fn match_started(&mut self, info: &MatchInfo) {
        self.keep.match_started(info);
    }

    fn on_tick(&mut self, view: &WorldView) -> Option<Ask> {
        self.decide(view)
    }

    fn on_events(&mut self, tick: u32, events: &[EventKind]) {
        self.keep.heard(tick, events, &self.params);
    }

    fn on_reject(&mut self, _seq: u32, _reason: RejectReason) {}

    fn finished(&mut self, _winner: Team, _stats: &MatchStats) {}
}

/// What the bot would have ordered, for anything comparing it with another.
pub fn ordered(want: Want) -> Order {
    want.order()
}
