//! What a match is played to and what runs in it, whatever map it is on.

use serde::{Deserialize, Serialize};

/// A choice of the three lanes. A map with fewer lanes has its one lane as
/// mid.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Lanes {
    /// The top lane.
    pub top: bool,
    /// The middle lane.
    pub mid: bool,
    /// The bottom lane.
    pub bot: bool,
}

impl Lanes {
    /// Every lane.
    pub const ALL: Lanes = Lanes {
        top: true,
        mid: true,
        bot: true,
    };
    /// No lane.
    pub const NONE: Lanes = Lanes {
        top: false,
        mid: false,
        bot: false,
    };
    /// The middle lane alone.
    pub const MID: Lanes = Lanes {
        top: false,
        mid: true,
        bot: false,
    };
    /// The top and the bottom lane.
    pub const SIDES: Lanes = Lanes {
        top: true,
        mid: false,
        bot: true,
    };
}

/// What a match is played to and what runs in it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rules {
    /// Lanes creep waves walk. A lane the map does not have sends none.
    pub waves: Lanes,
    /// Lanes whose towers take no harm.
    pub invulnerable_towers: Lanes,
    /// Hero deaths, counted over a side's seats, that lose it the match.
    /// Zero: deaths decide nothing.
    pub death_limit: u16,
    /// Whether losing any tower loses the match.
    pub tower_ends_it: bool,
    /// Ticks of play after the pregame before the match ends in a draw.
    /// Zero: no limit.
    pub time_limit: u32,
    /// Whether cheat commands are honoured.
    pub cheats: bool,
}
