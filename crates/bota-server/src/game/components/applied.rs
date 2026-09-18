//! Applied stat changes, held apart from the modifier set.

use bota_proto::ModifierSpec;

/// One applied stat change and how long it has left.
///
/// Nothing an ability, an item or a dispel does can reach it: only whatever
/// applied it replaces it, and only the tick countdown or the fall of the
/// body takes it away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppliedModifier {
    /// What it changes.
    pub spec: ModifierSpec,
    /// Ticks before it lifts. Absent for one that runs until the body falls.
    pub ticks_left: Option<u32>,
    /// Where it came from.
    pub origin: AppliedOrigin,
}

impl AppliedModifier {
    /// Runs one tick down; true when it has run out.
    pub fn tick(&mut self) -> bool {
        let Some(left) = &mut self.ticks_left else {
            return false;
        };
        *left = left.saturating_sub(1);
        *left == 0
    }
}

/// Where one applied change came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppliedOrigin {
    /// Trusted match setup, put on as the unit was stood up.
    Setup,
    /// A cheat order.
    Cheat,
}

/// Everything applied to one unit, one entry per source, so each keeps its
/// own countdown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppliedModifiers(Vec<AppliedModifier>);

impl AppliedModifiers {
    /// A set holding one entry.
    pub fn single(applied: AppliedModifier) -> Self {
        Self(vec![applied])
    }

    /// Adds one entry.
    pub fn push(&mut self, applied: AppliedModifier) {
        self.0.push(applied);
    }

    /// Every entry, in the order it was applied.
    pub fn iter(&self) -> impl Iterator<Item = &AppliedModifier> {
        self.0.iter()
    }

    /// How many entries are held.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// The first entry held.
    pub fn first(&self) -> Option<&AppliedModifier> {
        self.0.first()
    }

    /// Drops every entry the keeper turns down.
    pub fn retain(&mut self, keep: impl FnMut(&mut AppliedModifier) -> bool) {
        self.0.retain_mut(keep);
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
