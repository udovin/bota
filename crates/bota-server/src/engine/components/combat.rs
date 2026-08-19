//! An attack in progress.

use crate::engine::Entity;

/// Who an entity is set on.
///
/// Absent when it is set on nobody. Who puts it here is target acquisition,
/// the lane creep mind, or an order; the attack cycle only reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target(pub Entity);

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
