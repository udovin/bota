//! A blow on its way to being felt.

use bota_proto::DamageKind;

use crate::game::Entity;

/// Damage that has been dealt and not yet taken off anybody.
///
/// It stands on an entity of its own for the moment between the swing that
/// made it and the tick that resolves it. Nothing points at that entity and
/// nothing outlives the resolving, so it carries no place and no side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    /// Who dealt it, while that one still stands.
    pub source: Option<Entity>,
    /// Who takes it.
    pub target: Entity,
    /// Before armor and resistance.
    pub amount: i32,
    /// Which reduction applies.
    pub kind: DamageKind,
    /// Whether it was a critical strike.
    pub crit: bool,
}
