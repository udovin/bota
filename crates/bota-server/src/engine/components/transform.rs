//! Where an entity stands and how much room it takes.

use bota_proto::{Angle, Fixed, Vec2};

/// Where an entity is and which way it looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transform {
    /// Position on the map, in world units.
    pub pos: Vec2,
    /// The way it faces. Turning takes time, so this does not follow from the
    /// way it is walking.
    pub facing: Angle,
}

/// The circle an entity occupies. Absent for whatever nothing collides with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hull {
    /// Radius in world units.
    pub radius: Fixed,
}
