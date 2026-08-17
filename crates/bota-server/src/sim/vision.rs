//! Fog of war.
//!
//! Vision is a pure radius: a point is visible to a team when any of its
//! units with a vision radius stands close enough. Nothing is cached; the
//! world is small enough to ask directly.

use bota_proto::{EntityId, Fixed, Team, Vec2};

use crate::sim::World;

impl World {
    /// Whether a team currently sees a point on the map.
    pub fn can_see_point(&self, team: Team, pos: Vec2) -> bool {
        self.units.iter().any(|(_, u)| {
            u.team == team && u.vision_radius > Fixed::ZERO && u.pos.within(pos, u.vision_radius)
        })
    }

    /// Whether a team currently sees a unit.
    ///
    /// A team always sees its own units. Used to validate orders: a target the
    /// team cannot see may as well not exist.
    pub fn can_see(&self, team: Team, target: EntityId) -> bool {
        match self.units.get(target) {
            None => false,
            Some(unit) => unit.team == team || self.can_see_point(team, unit.pos),
        }
    }
}
