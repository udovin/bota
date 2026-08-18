//! The short pather: steering around standing bodies in continuous space.

use bota_proto::{EntityId, Fixed, Vec2};

use crate::sim::{Arena, Unit, isqrt64, rules, segment_distance_squared};

/// The immediate aim point of a walker heading for `waypoint`.
///
/// The waypoint itself when the straight segment is clear, otherwise a
/// tangent point around the first standing body in the way, resolved over a
/// few hops when tangents uncover further bodies. Walking bodies, bodies
/// beyond the steer range and whoever occupies the waypoint itself are not
/// steered around; running into those is resolved at contact.
pub fn steer_target(units: &Arena<Unit>, mover: EntityId, pos: Vec2, waypoint: Vec2) -> Vec2 {
    let Some(u) = units.get(mover) else {
        return waypoint;
    };
    let mut aim = waypoint;
    for _ in 0..rules::STEER_HOPS {
        let Some((center, inflated)) = first_stander(units, mover, u.radius, pos, aim, waypoint)
        else {
            return aim;
        };
        let Some((left, right)) = tangent_points(pos, center, inflated) else {
            return aim;
        };
        aim = shorter_way(pos, waypoint, left, right);
    }
    aim
}

/// The standing body nearest to `pos` whose inflated circle cuts the
/// segment `pos` to `aim`, if any is in steer range.
fn first_stander(
    units: &Arena<Unit>,
    mover: EntityId,
    radius: Fixed,
    pos: Vec2,
    aim: Vec2,
    waypoint: Vec2,
) -> Option<(Vec2, i64)> {
    let range = rules::units(rules::SHORT_PATH_RANGE);
    let mut best: Option<(i64, Vec2, i64)> = None;
    for (id, other) in units.iter() {
        if id == mover || other.moving {
            continue;
        }
        let inflated = radius + other.radius + rules::units(rules::STEER_MARGIN);
        if other.pos.within(waypoint, inflated) || !other.pos.within(pos, range) {
            continue;
        }
        let infl = i64::from(inflated.raw);
        if segment_distance_squared(other.pos, pos, aim) >= infl * infl {
            continue;
        }
        let d2 = pos.distance_squared(other.pos);
        if d2 <= infl * infl {
            continue; // already against the body: contact rules take over
        }
        if best.is_none_or(|(bd, _, _)| d2 < bd) {
            best = Some((d2, other.pos, infl));
        }
    }
    best.map(|(_, center, infl)| (center, infl))
}

/// The two tangent points from `pos` onto a circle, or nothing from inside.
fn tangent_points(pos: Vec2, center: Vec2, radius_raw: i64) -> Option<(Vec2, Vec2)> {
    let dx = i64::from(center.x.raw) - i64::from(pos.x.raw);
    let dy = i64::from(center.y.raw) - i64::from(pos.y.raw);
    let d2 = dx * dx + dy * dy;
    let r2 = radius_raw * radius_raw;
    if d2 <= r2 {
        return None;
    }
    let t2 = d2 - r2;
    let rt = radius_raw * isqrt64(t2);
    let coord = |num: i128| (num / i128::from(d2)) as i64;
    let point = |side: i128| Vec2 {
        x: Fixed {
            raw: (i64::from(pos.x.raw)
                + coord(i128::from(t2) * i128::from(dx) - side * i128::from(rt) * i128::from(dy)))
                as i32,
        },
        y: Fixed {
            raw: (i64::from(pos.y.raw)
                + coord(i128::from(t2) * i128::from(dy) + side * i128::from(rt) * i128::from(dx)))
                as i32,
        },
    };
    Some((point(1), point(-1)))
}

/// Whichever of two detour points makes the shorter trip to the waypoint.
/// Ties go to the first.
fn shorter_way(pos: Vec2, waypoint: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let trip =
        |via: Vec2| isqrt64(pos.distance_squared(via)) + isqrt64(via.distance_squared(waypoint));
    if trip(b) < trip(a) { b } else { a }
}
