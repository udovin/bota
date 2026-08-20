//! What a bot has to keep track of between ticks, whatever it decides with.
//!
//! None of this is a decision. It is the trees that are still standing, where
//! the attack cycle stands, and what was last said — the things a policy has
//! to be told and cannot read off one snapshot. The rule-driven bot keeps its
//! own; this is the same keeping for everything on the network's side, so that
//! what the network is shown while it plays is what it was shown while it
//! learned.

use bota_proto::{DamageKind, EntityId, EventKind, MatchInfo, SlotId, Vec2, WorldView};

use crate::{Ask, Params, Sight, Want};

/// Ticks a second, until the server says otherwise.
const TICK_RATE: f32 = 30.0;

/// The running state a policy needs and a snapshot does not carry.
#[derive(Clone, Debug, Default)]
pub struct Upkeep {
    /// Ticks a second, as the match runs.
    pub tick_rate: f32,
    /// Every tree the map grew.
    forest: Vec<Vec2>,
    /// The ones still standing, and how many were down when that was worked
    /// out.
    standing: Vec<Vec2>,
    felled: usize,
    /// The body being driven, so its own blows are known when they land.
    body: Option<EntityId>,
    /// The tick the next swing may begin on.
    ready_at: u32,
    /// Ticks between one swing beginning and the next.
    interval: u32,
    /// What was asked for last, and the tick it was asked on.
    said: Option<(Want, u32)>,
}

impl Upkeep {
    /// Nothing kept yet.
    pub fn new() -> Upkeep {
        Upkeep {
            tick_rate: TICK_RATE,
            forest: Vec::new(),
            standing: Vec::new(),
            felled: usize::MAX,
            body: None,
            ready_at: 0,
            interval: 30,
            said: None,
        }
    }

    /// Takes in what the match is.
    pub fn match_started(&mut self, info: &MatchInfo) {
        self.forest = info.trees.clone();
        self.standing.clear();
        self.felled = usize::MAX;
        self.tick_rate = f32::from(info.tick_rate.max(1));
    }

    /// The trees still standing.
    pub fn trees(&self) -> &[Vec2] {
        &self.standing
    }

    /// Forgets what belonged to a body that is no longer standing.
    pub fn forget(&mut self) {
        self.body = None;
        self.ready_at = 0;
        self.said = None;
    }

    /// The world through one seat's eyes, with the wait for the next swing
    /// filled in.
    pub fn look<'a>(&mut self, view: &'a WorldView, slot: SlotId) -> Option<Sight<'a>> {
        self.remember_the_forest(view);
        let sight = Sight::new(view, slot, self.tick_rate)?;
        self.body = Some(sight.me.id);
        self.interval = sight.me.attack_interval.max(1);
        Some(Sight {
            wait: self.ready_at.saturating_sub(view.tick) as f32,
            ..sight
        })
    }

    /// Notes its own blows landing, which is the only word the wire gives
    /// about where the attack cycle stands.
    pub fn heard(&mut self, tick: u32, events: &[EventKind], params: &Params) {
        let Some(me) = self.body else {
            return;
        };
        for event in events {
            if let EventKind::Damaged {
                source,
                kind: DamageKind::Physical,
                ..
            } = event
                && *source == Some(me)
            {
                self.ready_at = tick
                    .saturating_sub(params.swing_lead_ticks as u32)
                    .saturating_add(self.interval);
            }
        }
    }

    /// Puts a want on the wire, unless it is the one already standing.
    pub fn say(&mut self, want: Want, tick: u32, params: &Params) -> Option<Ask> {
        if let Some((said, when)) = self.said
            && want.same_as(said, params.resend_drift)
            && tick.saturating_sub(when) < params.resend_ticks.max(1.0) as u32
        {
            return None;
        }
        self.said = Some((want, tick));
        Some(want.ask())
    }

    /// Keeps the list of standing trees in step with what has been felled.
    fn remember_the_forest(&mut self, view: &WorldView) {
        if self.felled == view.felled_trees.len() {
            return;
        }
        self.felled = view.felled_trees.len();
        self.standing = self
            .forest
            .iter()
            .enumerate()
            .filter(|(at, _)| !view.felled_trees.contains(&(*at as u32)))
            .map(|(_, tree)| *tree)
            .collect();
    }
}
