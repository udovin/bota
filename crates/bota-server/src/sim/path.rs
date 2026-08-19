//! Grid pathfinding around structures.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bota_proto::Vec2;

use crate::sim::{PassGrid, rules};

/// Whether the straight segment crosses only walkable cells.
///
/// Sampled every half-cell along the line, which cannot skip over a cell at
/// that spacing.
pub fn grid_los(grid: &PassGrid, from: Vec2, to: Vec2) -> bool {
    let dx = i64::from(to.x.raw) - i64::from(from.x.raw);
    let dy = i64::from(to.y.raw) - i64::from(from.y.raw);
    let sample = i64::from(rules::GRID_CELL_SIZE) << 15; // half a cell, raw
    let len = dx.abs().max(dy.abs());
    let steps = (len / sample + 1).max(1);
    for i in 0..=steps {
        let p = Vec2 {
            x: bota_proto::Fixed {
                raw: (i64::from(from.x.raw) + dx * i / steps) as i32,
            },
            y: bota_proto::Fixed {
                raw: (i64::from(from.y.raw) + dy * i / steps) as i32,
            },
        };
        if !grid.walkable(p) {
            return false;
        }
    }
    true
}

const CELLS: usize = rules::GRID_CELLS;
const STRAIGHT: u32 = 64;
const DIAGONAL: u32 = 90;

fn heuristic(a: (usize, usize), b: (usize, usize)) -> u32 {
    let dx = a.0.abs_diff(b.0) as u32;
    let dy = a.1.abs_diff(b.1) as u32;
    STRAIGHT * dx.max(dy) + (DIAGONAL - STRAIGHT) * dx.min(dy)
}

/// The nearest spot a unit may actually stand on, for a point that may sit
/// inside a building's footprint.
pub fn nearest_open(grid: &PassGrid, at: Vec2) -> Vec2 {
    PassGrid::cell_of(at)
        .and_then(|cell| routable_cell(grid, cell))
        .map_or(at, PassGrid::cell_center)
}

/// The open cell to route to for a goal, stepping to a neighbour when the
/// goal cell itself is blocked.
fn routable_cell(grid: &PassGrid, cell: (usize, usize)) -> Option<(usize, usize)> {
    if grid.cell_open(cell.0, cell.1) {
        return Some(cell);
    }
    for (dx, dy) in NEIGHBOURS {
        let nx = cell.0 as i32 + dx;
        let ny = cell.1 as i32 + dy;
        if nx >= 0
            && ny >= 0
            && (nx as usize) < CELLS
            && (ny as usize) < CELLS
            && grid.cell_open(nx as usize, ny as usize)
        {
            return Some((nx as usize, ny as usize));
        }
    }
    None
}

const NEIGHBOURS: [(i32, i32); 8] = [
    (1, 0),
    (0, 1),
    (-1, 0),
    (0, -1),
    (1, 1),
    (-1, 1),
    (-1, -1),
    (1, -1),
];

/// A* over the passability grid, returning the corner waypoints of the route.
///
/// Empty when no route exists or none is needed. Diagonal steps never cut a
/// blocked corner. Ties break on cell index, so the route is the same on
/// every platform.
pub fn find_path(grid: &PassGrid, from: Vec2, to: Vec2) -> Vec<Vec2> {
    let (Some(start), Some(goal)) = (PassGrid::cell_of(from), PassGrid::cell_of(to)) else {
        return Vec::new();
    };
    let (Some(start), Some(goal)) = (routable_cell(grid, start), routable_cell(grid, goal)) else {
        return Vec::new();
    };
    if start == goal {
        return Vec::new();
    }
    let idx = |c: (usize, usize)| c.1 * CELLS + c.0;
    let mut best = vec![u32::MAX; CELLS * CELLS];
    let mut parent = vec![u32::MAX; CELLS * CELLS];
    let mut heap = BinaryHeap::new();
    best[idx(start)] = 0;
    heap.push(Reverse((heuristic(start, goal), idx(start) as u32)));
    while let Some(Reverse((_, at))) = heap.pop() {
        let at = at as usize;
        let cell = (at % CELLS, at / CELLS);
        if cell == goal {
            break;
        }
        let g = best[at];
        for (i, (dx, dy)) in NEIGHBOURS.iter().enumerate() {
            let nx = cell.0 as i32 + dx;
            let ny = cell.1 as i32 + dy;
            if nx < 0 || ny < 0 || nx as usize >= CELLS || ny as usize >= CELLS {
                continue;
            }
            let next = (nx as usize, ny as usize);
            if !grid.cell_open(next.0, next.1) {
                continue;
            }
            let diagonal = i >= 4;
            if diagonal && (!grid.cell_open(next.0, cell.1) || !grid.cell_open(cell.0, next.1)) {
                continue; // no cutting a blocked corner
            }
            let cost = g + if diagonal { DIAGONAL } else { STRAIGHT };
            let ni = idx(next);
            if cost < best[ni] {
                best[ni] = cost;
                parent[ni] = at as u32;
                heap.push(Reverse((cost + heuristic(next, goal), ni as u32)));
            }
        }
    }
    if parent[idx(goal)] == u32::MAX {
        return Vec::new();
    }
    // Walk the parents back, then keep only the corners.
    let mut cells = vec![goal];
    let mut at = idx(goal);
    while at != idx(start) {
        at = parent[at] as usize;
        cells.push((at % CELLS, at / CELLS));
    }
    cells.reverse();
    let mut corners: Vec<(usize, usize)> = Vec::new();
    for i in 1..cells.len() {
        let dir = (
            cells[i].0 as i32 - cells[i - 1].0 as i32,
            cells[i].1 as i32 - cells[i - 1].1 as i32,
        );
        let prev_dir = if i >= 2 {
            Some((
                cells[i - 1].0 as i32 - cells[i - 2].0 as i32,
                cells[i - 1].1 as i32 - cells[i - 2].1 as i32,
            ))
        } else {
            None
        };
        if prev_dir.is_some_and(|p| p != dir)
            && let Some(&last) = cells.get(i - 1)
        {
            corners.push(last);
        }
    }
    corners.push(goal);
    corners.iter().map(|&c| PassGrid::cell_center(c)).collect()
}
