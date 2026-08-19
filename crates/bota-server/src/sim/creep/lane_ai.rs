//! What a lane creep is doing besides fighting.

use bota_proto::Vec2;

use crate::sim::TraceSide;

/// The autonomous behaviour of a creep, by kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreepAi {
    /// A lane creep marching its lane.
    Lane(LaneCreepAi),
    /// A neutral creep guarding its camp.
    Neutral(crate::sim::NeutralAi),
}

/// A lane creep's march, chase and way back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaneCreepAi {
    /// How many waypoints of its route are behind it.
    pub step: u16,
    /// Where it left the route. Absent while it is on the route.
    pub anchor: Option<Vec2>,
    /// Ticks left of the chase after the target left acquisition range.
    /// Zero once the chase is spent.
    pub chase_left: u32,
    /// Ticks left in which the ordinary ranking may not take the creep off
    /// the target an attack order handed it. Zero when the ranking rules.
    pub provoked: u32,
    /// Where a target was last seen before the fog took it. Walked to, then
    /// forgotten.
    pub last_seen: Option<Vec2>,
    /// Which way round the body in its way it settled on, kept while that
    /// body is still in the way.
    pub trace: Option<TraceSide>,
}

impl LaneCreepAi {
    /// A creep fresh out of the barracks, at the start of its route.
    pub fn new() -> LaneCreepAi {
        LaneCreepAi {
            step: 0,
            anchor: None,
            chase_left: 0,
            provoked: 0,
            last_seen: None,
            trace: None,
        }
    }
}

impl Default for LaneCreepAi {
    fn default() -> LaneCreepAi {
        LaneCreepAi::new()
    }
}
