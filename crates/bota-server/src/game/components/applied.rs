//! Cheat-granted stat changes, held apart from the modifier set.

use bota_proto::ModifierSpec;

/// One cheat-granted stat change on a unit and the ticks it has left.
///
/// Nothing an ability, an item or a dispel does can reach it: only the cheat
/// that put it on replaces it and only the tick countdown takes it away. A
/// fallen body takes its own with it and a respawned one carries none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppliedModifier {
    /// What it changes.
    pub spec: ModifierSpec,
    /// Ticks before it lifts. At least one while it is held.
    pub ticks_left: u32,
}
