//! A missile in flight.

use bota_proto::{AbilityId, DamageKind};

use crate::game::Entity;

/// A missile on its way to somebody.
///
/// It carries a [`Transform`] for where it is and a team of its own; whoever
/// threw it may be gone by the time it lands.
///
/// [`Transform`]: crate::game::Transform
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Projectile {
    /// World units per second.
    pub speed: bota_proto::Fixed,
    /// Who threw it. Absent once that entity is gone.
    pub source: Option<Entity>,
    /// Who it is aimed at.
    pub target: Entity,
    /// Damage it lands.
    pub damage: i32,
    /// Which reduction applies.
    pub kind: DamageKind,
    /// The ability behind it, if it was not a plain attack.
    pub ability: Option<AbilityId>,
    /// Tier of the building that threw it, zero for anything else.
    pub launch_tier: u8,
    /// Whether the hit is a critical strike.
    pub crit: bool,
    /// Bounces it has left.
    pub bounces_left: u8,
    /// Who it has already struck, so it does not strike them twice.
    pub bounced: Vec<Entity>,
}
