//! The maps a match may be played on.
//!
//! Everything that differs between them lives here: where the buildings
//! stand, where the lanes run, which camps the jungle holds, and whether the
//! ground is the real terrain or open field.

use bota_proto::{MapId, Vec2};

use crate::game::{CampDef, CampKind, rules};

/// Where a map's ground comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terrain {
    /// The baked Dota terrain: cliffs, ramps, the river.
    Dota,
    /// Open field at one elevation, walkable everywhere.
    Flat,
}

/// One playable map.
#[derive(Clone, Copy, Debug)]
pub struct MapDef {
    /// Which map this is on the wire.
    pub id: MapId,
    /// Fountain centres, Radiant first.
    pub fountains: [Vec2; 2],
    /// Ancient positions, Radiant first.
    pub ancients: [Vec2; 2],
    /// Radiant towers as lane, tier and position.
    pub radiant_towers: &'static [(u8, u8, Vec2)],
    /// Dire towers as lane, tier and position.
    pub dire_towers: &'static [(u8, u8, Vec2)],
    /// Where waves appear, by team then lane.
    pub creep_spawns: [[Vec2; 3]; 2],
    /// How many lanes the map runs, from lane zero up.
    pub lanes: u8,
    /// Corners a lane bends through between the two tier-one towers, by lane
    /// index. Empty for a lane that runs straight.
    pub lane_corners: &'static [&'static [Vec2]],
    /// The jungle camps.
    pub camps: &'static [CampDef],
    /// Whether the map carries the Dota forest.
    pub trees: bool,
    /// Which ground it stands on.
    pub terrain: Terrain,
}

/// The Dota lanes bend once each on the way round the map; mid runs straight.
const DOTA_CORNERS: &[&[Vec2]] = &[&[], &[rules::TOP_CORNER], &[rules::BOT_CORNER]];

/// The mini map runs one straight lane, the two sides mirrored about the
/// point halfway between the tier ones.
const MINI_CORNERS: &[&[Vec2]] = &[&[]];

const fn camp(pos: Vec2, kind: CampKind, pullable: bool, flooded: bool) -> CampDef {
    CampDef {
        pos,
        kind,
        pullable,
        flooded,
    }
}

/// The mini map's two camps, one a side, both beside the lane and pullable.
const MINI_CAMPS: [CampDef; 2] = [
    camp(Vec2::from_ints(7200, 8600), CampKind::Small, true, false),
    camp(Vec2::from_ints(11200, 9800), CampKind::Small, true, false),
];

const MINI_RADIANT_TOWERS: &[(u8, u8, Vec2)] = &[
    (rules::LANE_MID, 1, Vec2::from_ints(9600, 9216)),
    (rules::LANE_MID, 2, Vec2::from_ints(8400, 9216)),
    (rules::LANE_MID, 3, Vec2::from_ints(7200, 9216)),
];

const MINI_DIRE_TOWERS: &[(u8, u8, Vec2)] = &[
    (rules::LANE_MID, 1, Vec2::from_ints(10800, 9216)),
    (rules::LANE_MID, 2, Vec2::from_ints(12000, 9216)),
    (rules::LANE_MID, 3, Vec2::from_ints(13200, 9216)),
];

/// Every map, indexed by [`MapId`].
pub const MAPS: [MapDef; 2] = [
    // The real Dota map.
    MapDef {
        id: MapId(0),
        fountains: [rules::RADIANT_FOUNTAIN_POS, rules::DIRE_FOUNTAIN_POS],
        ancients: [rules::RADIANT_ANCIENT_POS, rules::DIRE_ANCIENT_POS],
        radiant_towers: &rules::RADIANT_TOWERS,
        dire_towers: &rules::DIRE_TOWERS,
        creep_spawns: [rules::RADIANT_CREEP_SPAWNS, rules::DIRE_CREEP_SPAWNS],
        lanes: 3,
        lane_corners: DOTA_CORNERS,
        camps: &crate::game::CAMPS,
        trees: true,
        terrain: Terrain::Dota,
    },
    // A straight lane on open ground, for reading behaviour off quickly.
    MapDef {
        id: MapId(1),
        fountains: [Vec2::from_ints(5600, 9216), Vec2::from_ints(14800, 9216)],
        ancients: [Vec2::from_ints(6400, 9216), Vec2::from_ints(14000, 9216)],
        radiant_towers: MINI_RADIANT_TOWERS,
        dire_towers: MINI_DIRE_TOWERS,
        creep_spawns: [
            [Vec2::from_ints(6800, 9216); 3],
            [Vec2::from_ints(13600, 9216); 3],
        ],
        lanes: 1,
        lane_corners: MINI_CORNERS,
        camps: &MINI_CAMPS,
        trees: false,
        terrain: Terrain::Flat,
    },
];

/// The map a match is played on. An unknown id falls back on the Dota map.
pub fn map_of(id: MapId) -> &'static MapDef {
    MAPS.iter().find(|m| m.id == id).unwrap_or(&MAPS[0])
}

impl MapDef {
    /// Every lane the map runs.
    pub fn lanes(&self) -> impl Iterator<Item = u8> {
        0..self.lanes
    }

    /// Where this map's index sits in the per-team tables.
    pub fn index(&self) -> usize {
        self.id.0 as usize
    }
}
