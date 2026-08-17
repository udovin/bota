//! Deterministic integer geometry: stepping, separation, facing, bounds.

use bota_proto::{Angle, EntityId, Fixed, Vec2};

use crate::sim::{Arena, Unit, rules};

/// Integer square root, rounded down.
///
/// The one place a length is ever taken; everything else compares squares.
pub fn isqrt64(n: i64) -> i64 {
    debug_assert!(n >= 0, "no square root of a negative");
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut next = (x + 1) / 2;
    while next < x {
        x = next;
        next = (x + n / x) / 2;
    }
    x
}

/// The distance covered in one tick at a per-second speed.
pub fn per_tick(speed: Fixed) -> Fixed {
    Fixed {
        raw: speed.raw / rules::TICKS_PER_SECOND as i32,
    }
}

/// One movement step from `pos` towards `target`, no further than `step`.
pub fn move_towards(pos: Vec2, target: Vec2, step: Fixed) -> Vec2 {
    let dx = i64::from(target.x.raw) - i64::from(pos.x.raw);
    let dy = i64::from(target.y.raw) - i64::from(pos.y.raw);
    let dist = isqrt64(dx * dx + dy * dy);
    if dist <= i64::from(step.raw) {
        return target;
    }
    let sx = dx * i64::from(step.raw) / dist;
    let sy = dy * i64::from(step.raw) / dist;
    Vec2 {
        x: Fixed {
            raw: pos.x.raw.saturating_add(sx as i32),
        },
        y: Fixed {
            raw: pos.y.raw.saturating_add(sy as i32),
        },
    }
}

/// The facing from one position towards another, in brads.
///
/// A piecewise-linear octant approximation: exact on the axes and diagonals,
/// within a few degrees elsewhere. Facing is cosmetic, nothing compares it.
pub fn facing_towards(from: Vec2, to: Vec2) -> Angle {
    let dx = i64::from(to.x.raw) - i64::from(from.x.raw);
    let dy = i64::from(to.y.raw) - i64::from(from.y.raw);
    if dx == 0 && dy == 0 {
        return Angle { brads: 0 };
    }
    let (adx, ady) = (dx.abs(), dy.abs());
    // 8192 brads per octant; the slope maps linearly onto one octant.
    let slope = if adx >= ady {
        (ady << 13) / adx
    } else {
        (adx << 13) / ady
    };
    let octant = match (dx >= 0, dy >= 0, adx >= ady) {
        (true, true, true) => slope,
        (true, true, false) => 16384 - slope,
        (false, true, false) => 16384 + slope,
        (false, true, true) => 32768 - slope,
        (false, false, true) => 32768 + slope,
        (false, false, false) => 49152 - slope,
        (true, false, false) => 49152 + slope,
        (true, false, true) => 65536 - slope,
    };
    Angle {
        brads: (octant & 0xFFFF) as u16,
    }
}

/// Keeps a position on the map.
pub fn clamp_to_map(pos: Vec2) -> Vec2 {
    let max = Fixed::from_int(rules::MAP_SIZE);
    Vec2 {
        x: Fixed {
            raw: pos.x.raw.clamp(0, max.raw),
        },
        y: Fixed {
            raw: pos.y.raw.clamp(0, max.raw),
        },
    }
}

/// The walkability grid of the map.
///
/// One bit per cell, `true` is walkable. The v0.1 map is open ground, so every
/// cell starts walkable; buildings do not occupy cells, they collide by radius.
#[derive(Clone, Debug)]
pub struct PassGrid {
    /// One row per entry, one cell per bit.
    rows: [u128; rules::GRID_CELLS],
}

impl PassGrid {
    /// A fully walkable map.
    pub fn open() -> PassGrid {
        PassGrid {
            rows: [u128::MAX; rules::GRID_CELLS],
        }
    }

    /// Whether a position is on the map and walkable.
    pub fn walkable(&self, pos: Vec2) -> bool {
        if pos.x.raw < 0 || pos.y.raw < 0 {
            return false;
        }
        let cx = pos.x.to_int() / rules::GRID_CELL_SIZE;
        let cy = pos.y.to_int() / rules::GRID_CELL_SIZE;
        if cx >= rules::GRID_CELLS as i32 || cy >= rules::GRID_CELLS as i32 {
            return false;
        }
        self.rows[cy as usize] & (1 << cx) != 0
    }
}

/// Pushes overlapping units apart.
///
/// Pairs resolve in ascending id order, one pass per tick; a deep overlap
/// finishes separating over the following ticks. Two movable units split the
/// push, a movable unit yields fully to a structure.
pub fn separate_collisions(units: &mut Arena<Unit>) {
    let ids: Vec<EntityId> = units.ids();
    for (i, &a_id) in ids.iter().enumerate() {
        for &b_id in &ids[i + 1..] {
            let (Some(a), Some(b)) = (units.get(a_id), units.get(b_id)) else {
                continue;
            };
            let a_static = a.move_speed == Fixed::ZERO;
            let b_static = b.move_speed == Fixed::ZERO;
            if a_static && b_static {
                continue;
            }
            let dx = i64::from(b.pos.x.raw) - i64::from(a.pos.x.raw);
            let dy = i64::from(b.pos.y.raw) - i64::from(a.pos.y.raw);
            let min_dist = i64::from((a.radius + b.radius).raw);
            let dist2 = dx * dx + dy * dy;
            if dist2 >= min_dist * min_dist {
                continue;
            }
            let dist = isqrt64(dist2);
            // Perfectly stacked units get a deterministic axis to part along.
            let (ux, uy, dist) = if dist == 0 {
                (1 << Fixed::FRAC_BITS, 0, 1 << Fixed::FRAC_BITS)
            } else {
                (dx, dy, dist)
            };
            let overlap = min_dist - dist;
            let (push_a, push_b) = if a_static {
                (0, overlap)
            } else if b_static {
                (overlap, 0)
            } else {
                (overlap / 2, overlap - overlap / 2)
            };
            if push_a > 0 {
                let unit = units.get_mut(a_id).expect("checked above");
                unit.pos = clamp_to_map(Vec2 {
                    x: Fixed {
                        raw: unit.pos.x.raw.saturating_sub((ux * push_a / dist) as i32),
                    },
                    y: Fixed {
                        raw: unit.pos.y.raw.saturating_sub((uy * push_a / dist) as i32),
                    },
                });
            }
            if push_b > 0 {
                let unit = units.get_mut(b_id).expect("checked above");
                unit.pos = clamp_to_map(Vec2 {
                    x: Fixed {
                        raw: unit.pos.x.raw.saturating_add((ux * push_b / dist) as i32),
                    },
                    y: Fixed {
                        raw: unit.pos.y.raw.saturating_add((uy * push_b / dist) as i32),
                    },
                });
            }
        }
    }
}
