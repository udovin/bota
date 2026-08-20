//! The ground: elevation tiers, water and terrain walkability.

use bota_proto::Vec2;

use crate::game::{TERRAIN_CELLS, TERRAIN_RLE, rules};

/// The decoded terrain, one byte per passability cell.
///
/// Bit 7 is ground the map's own gridnav calls walkable, bit 6 is river
/// water, the low bits are the elevation tier in 128-unit steps.
#[derive(Clone, Debug)]
pub struct Ground {
    cells: Vec<u8>,
}

impl Ground {
    /// Decodes the baked terrain.
    pub fn decode() -> Ground {
        let mut cells = Vec::with_capacity(TERRAIN_CELLS * TERRAIN_CELLS);
        for &(run, value) in TERRAIN_RLE {
            for _ in 0..run {
                cells.push(value);
            }
        }
        debug_assert_eq!(cells.len(), TERRAIN_CELLS * TERRAIN_CELLS);
        Ground { cells }
    }

    /// Open field: walkable everywhere, one elevation, no water.
    pub fn flat() -> Ground {
        Ground {
            cells: vec![0x81; TERRAIN_CELLS * TERRAIN_CELLS],
        }
    }

    /// The ground a map stands on.
    pub fn of(map: &crate::game::MapDef) -> Ground {
        match map.terrain {
            crate::game::Terrain::Dota => Ground::decode(),
            crate::game::Terrain::Flat => Ground::flat(),
        }
    }

    fn cell(&self, pos: Vec2) -> u8 {
        let cx = pos.x.to_int() / rules::GRID_CELL_SIZE;
        let cy = pos.y.to_int() / rules::GRID_CELL_SIZE;
        if cx < 0 || cy < 0 || cx as usize >= TERRAIN_CELLS || cy as usize >= TERRAIN_CELLS {
            return 0;
        }
        self.cells[cy as usize * TERRAIN_CELLS + cx as usize]
    }

    /// The elevation tier under a position. Higher is higher ground.
    pub fn tier(&self, pos: Vec2) -> u8 {
        self.cell(pos) & 0x1f
    }

    /// Whether a position lies in river water.
    pub fn water(&self, pos: Vec2) -> bool {
        self.cell(pos) & 0x40 != 0
    }

    /// Whether the terrain itself allows standing on a cell.
    pub fn cell_walkable(&self, cx: usize, cy: usize) -> bool {
        cx < TERRAIN_CELLS && cy < TERRAIN_CELLS && self.cells[cy * TERRAIN_CELLS + cx] & 0x80 != 0
    }

    /// The wire form of a map's terrain: run-length pairs.
    pub fn wire_rle(map: &crate::game::MapDef) -> Vec<(u16, u8)> {
        match map.terrain {
            crate::game::Terrain::Dota => TERRAIN_RLE.to_vec(),
            crate::game::Terrain::Flat => {
                let mut out = Vec::new();
                let mut left = TERRAIN_CELLS * TERRAIN_CELLS;
                while left > 0 {
                    let run = left.min(u16::MAX as usize);
                    out.push((run as u16, 0x81));
                    left -= run;
                }
                out
            }
        }
    }
}
