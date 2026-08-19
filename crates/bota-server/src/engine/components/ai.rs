//! What a creep keeps about the fight it is in and the ground it holds.

use bota_proto::Vec2;

/// What a lane creep keeps while it is chasing or coming back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaneAi {
    /// Where it left its route. Absent while it is on the route.
    pub anchor: Option<Vec2>,
    /// Ticks of chase left before it gives its target up.
    pub chase_left: u32,
    /// Ticks left of the hold an attack order put on its target.
    pub provoked: u32,
    /// Where its target was last seen. Absent when it never lost one.
    pub last_seen: Option<Vec2>,
}

/// Which camp a neutral belongs to and where in it it stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CampHome {
    /// The camp's number on the map.
    pub camp: u8,
    /// The spot it walks back to.
    pub home: Vec2,
}

/// What a neutral keeps about being drawn away from its camp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NeutralAi {
    /// Ticks it may still be led before it turns for home.
    pub leash_left: u32,
    /// Ticks before it will take a target again.
    pub reaggro_block: u32,
    /// Ticks until its next chance to notice anybody.
    pub next_window: u32,
    /// Whether it is walking home and taking nothing on.
    pub going_home: bool,
}
