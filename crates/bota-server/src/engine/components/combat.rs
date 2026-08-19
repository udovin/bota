//! An attack in progress.

use crate::engine::Entity;

/// An attack begun and not yet landed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Windup {
    /// Who the attack is aimed at.
    pub target: Entity,
    /// Ticks until the hit lands or the missile leaves.
    pub ticks_left: u32,
}

/// Where an entity is in its attack cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attacking {
    /// The swing under way, if there is one.
    pub windup: Option<Windup>,
    /// Ticks before it may start another attack.
    pub cooldown: u32,
    /// Ticks of backswing left, during which it does not walk.
    pub recovering: u32,
}
