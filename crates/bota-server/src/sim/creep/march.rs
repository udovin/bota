//! How a creep walks: its own march, nothing like a hero's.
//!
//! A creep never plans around a body. It walks at the next waypoint of its
//! route until it touches something, and only then works its way round, one
//! side chosen and kept. What that costs it is the turning: it cannot face
//! the way round until it has turned, and it does not move while it is more
//! than [`rules::TURN_TOLERANCE_BRADS`] off.

use bota_proto::{EntityId, Fixed, Vec2};

use crate::sim::{
    Arena, PassGrid, Unit, blocked_by_units, clamp_to_map, isqrt64, move_towards, rules,
};

/// Which way round a body a creep decided to go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceSide {
    /// To the left of the line to the body.
    Left,
    /// To the right of it.
    Right,
}

impl TraceSide {
    fn sign(self) -> i64 {
        match self {
            TraceSide::Left => 1,
            TraceSide::Right => -1,
        }
    }
}

/// The waypoint a creep marches at, having cleared every point it has
/// already reached.
///
/// A creep aims at its next waypoint and nothing else: it is never pulled
/// sideways towards the centreline, and a waypoint counts as reached from
/// anywhere inside [`rules::LANE_WAYPOINT_RADIUS`]. Several waypoints may
/// fall inside that radius at once, and all of them are cleared together.
pub fn advance_waypoint(route: &[Vec2], from: usize, at: Vec2) -> usize {
    let radius = rules::units(rules::LANE_WAYPOINT_RADIUS);
    let mut step = from.min(route.len().saturating_sub(1));
    while step + 1 < route.len() && at.within(route[step], radius) {
        step += 1;
    }
    step
}

/// Where a creep aims this tick: the waypoint, or a way round the body in
/// the way.
///
/// Returns the aim and the side it settled on, so the creep keeps going the
/// same way round rather than dithering at the boundary.
///
/// A side is only taken if a step that way is one [`march_step`] would also
/// walk: closed ground counts as shut, so a creep beside a building does not
/// settle on the side that runs into it.
pub fn march_aim(
    units: &Arena<Unit>,
    grid: &PassGrid,
    mover: EntityId,
    waypoint: Vec2,
    step: Fixed,
    holding: Option<TraceSide>,
    shoving: bool,
) -> (Vec2, Option<TraceSide>) {
    let Some(unit) = units.get(mover) else {
        return (waypoint, None);
    };
    let pos = unit.pos;
    let straight = clamp_to_map(move_towards(pos, waypoint, step));
    if step_walkable(units, grid, mover, pos, straight, shoving) {
        // The way is open, so whatever it was going round is behind it now.
        return (waypoint, None);
    }
    // The side is chosen on first touch and kept while the way ahead is still
    // shut, so the creep does not dither at the boundary. Within that side the
    // aim swings further off the direct line until a step fits, and the last
    // resort is straight back: a knot has to be able to come apart.
    let first = holding.unwrap_or_else(|| {
        blocking_body(units, mover, straight)
            .map_or(TraceSide::Left, |b| pick_side(pos, b, waypoint))
    });
    let reach = i64::from((unit.radius + rules::units(rules::TRACE_CLEARANCE)).raw) * 2;
    let dx = i64::from(waypoint.x.raw) - i64::from(pos.x.raw);
    let dy = i64::from(waypoint.y.raw) - i64::from(pos.y.raw);
    for turn in [Turn::Eighth, Turn::Quarter, Turn::ThreeEighths] {
        for side in [first, other_side(first)] {
            let (tx, ty) = turn.apply(dx, dy, side.sign());
            let aim = aim_at(pos, tx, ty, reach);
            let ahead = clamp_to_map(move_towards(pos, aim, step));
            if ahead != pos && step_walkable(units, grid, mover, pos, ahead, shoving) {
                return (aim, Some(side));
            }
        }
    }
    (aim_at(pos, -dx, -dy, reach), Some(first))
}

/// How far off the line to the waypoint an aim is swung.
#[derive(Clone, Copy)]
enum Turn {
    /// An eighth of a turn, 45 degrees.
    Eighth,
    /// A quarter turn, square across the line.
    Quarter,
    /// Three eighths, 135 degrees: past square, but still forward of back.
    ThreeEighths,
}

impl Turn {
    /// The direction swung this far off `(dx, dy)`, `sign` picking the way
    /// round. The length is not kept; only the direction is used.
    fn apply(self, dx: i64, dy: i64, sign: i64) -> (i64, i64) {
        match self {
            Turn::Eighth => (dx - sign * dy, dy + sign * dx),
            Turn::Quarter => (-sign * dy, sign * dx),
            Turn::ThreeEighths => (-sign * dy - dx, sign * dx - dy),
        }
    }
}

/// A point `reach` away from `pos` along `(dx, dy)`.
fn aim_at(pos: Vec2, dx: i64, dy: i64, reach: i64) -> Vec2 {
    let len = isqrt64(dx * dx + dy * dy);
    if len == 0 {
        return pos;
    }
    clamp_to_map(Vec2 {
        x: Fixed {
            raw: pos.x.raw.saturating_add((dx * reach / len) as i32),
        },
        y: Fixed {
            raw: pos.y.raw.saturating_add((dy * reach / len) as i32),
        },
    })
}

/// Whether a step is one the creep may actually take, ground and bodies both.
///
/// A creep that has been shoving counts bodies as passable and is parted from
/// them afterwards. Ground is solid either way.
fn step_walkable(
    units: &Arena<Unit>,
    grid: &PassGrid,
    mover: EntityId,
    pos: Vec2,
    next: Vec2,
    shoving: bool,
) -> bool {
    if !grid.walkable(next) && grid.walkable(pos) {
        return false;
    }
    shoving || !blocked_by_units(units, mover, pos, next)
}

fn other_side(side: TraceSide) -> TraceSide {
    match side {
        TraceSide::Left => TraceSide::Right,
        TraceSide::Right => TraceSide::Left,
    }
}

/// The body a straight step would walk into, nearest first.
fn blocking_body(units: &Arena<Unit>, mover: EntityId, straight: Vec2) -> Option<Vec2> {
    let unit = units.get(mover)?;
    units
        .iter()
        .filter(|(id, other)| {
            *id != mover && {
                let min = i64::from((unit.radius + other.radius).raw);
                straight.distance_squared(other.pos) < min * min
            }
        })
        .min_by_key(|(id, other)| (unit.pos.distance_squared(other.pos), *id))
        .map(|(_, other)| other.pos)
}

/// Which way round leaves the creep nearer its waypoint. Ties go left.
fn pick_side(pos: Vec2, blocker: Vec2, waypoint: Vec2) -> TraceSide {
    let bx = i64::from(blocker.x.raw) - i64::from(pos.x.raw);
    let by = i64::from(blocker.y.raw) - i64::from(pos.y.raw);
    let wx = i64::from(waypoint.x.raw) - i64::from(pos.x.raw);
    let wy = i64::from(waypoint.y.raw) - i64::from(pos.y.raw);
    // Which side of the line to the body the waypoint lies on.
    if bx * wy - by * wx < 0 {
        TraceSide::Right
    } else {
        TraceSide::Left
    }
}

/// One step of the march, once the creep is facing near enough to walk.
///
/// Bodies are solid: a step that would enter a hull is refused and the creep
/// does not move that tick. A creep that has stood for
/// [`rules::MARCH_SHOVE_TICKS`] shoves instead, walking into the hull and
/// being parted from it after.
pub fn march_step(
    units: &Arena<Unit>,
    grid: &PassGrid,
    mover: EntityId,
    aim: Vec2,
    step: Fixed,
    shoving: bool,
) -> Vec2 {
    let Some(unit) = units.get(mover) else {
        return aim;
    };
    let pos = unit.pos;
    let next = clamp_to_map(move_towards(pos, aim, step));
    if !step_walkable(units, grid, mover, pos, next, shoving) {
        return pos;
    }
    next
}
