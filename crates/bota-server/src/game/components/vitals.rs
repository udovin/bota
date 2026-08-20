//! Health and mana.

use bota_proto::Fixed;

/// Health an entity has left.
///
/// Held finer than a whole point, so regeneration of less than one a tick
/// still adds up. What is shown is the whole part.
///
/// The maximum lives in [`Stats`], along with everything else worked out
/// afresh each tick.
///
/// [`Stats`]: crate::game::Stats
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    /// Health now. At or below zero the entity is dead.
    pub hp: Fixed,
}

/// Mana an entity has left, held the same way as [`Health`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mana {
    /// Mana now.
    pub mana: Fixed,
}
