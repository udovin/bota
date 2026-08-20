//! Float geometry over the fixed-point spots the wire carries.
//!
//! The bot is free to think in float: what is recorded are its orders, and an
//! order carries a spot, not the reasoning that arrived at it. Everything here
//! takes and returns [`Vec2`] so that the rounding happens in one place.

use bota_proto::{Angle, Vec2};

/// The world spans this many units on each axis.
pub const MAP_SIZE: f32 = 18432.0;

/// A spot as a pair of floats.
pub fn xy(at: Vec2) -> (f32, f32) {
    (at.x.to_f32(), at.y.to_f32())
}

/// The spot a pair of floats names, kept on the map.
pub fn spot(x: f32, y: f32) -> Vec2 {
    Vec2::from_ints(
        x.clamp(0.0, MAP_SIZE).round() as i32,
        y.clamp(0.0, MAP_SIZE).round() as i32,
    )
}

/// How far apart two spots are.
pub fn span(one: Vec2, other: Vec2) -> f32 {
    let (dx, dy) = (
        one.x.to_f32() - other.x.to_f32(),
        one.y.to_f32() - other.y.to_f32(),
    );
    (dx * dx + dy * dy).sqrt()
}

/// The spot a given distance from one towards another.
///
/// A distance past the far end overshoots it; a negative one goes the other
/// way. Two spots that are one and the same give that spot back.
pub fn along(from: Vec2, to: Vec2, distance: f32) -> Vec2 {
    let (fx, fy) = xy(from);
    let (tx, ty) = xy(to);
    let (dx, dy) = (tx - fx, ty - fy);
    let length = (dx * dx + dy * dy).sqrt();
    if length <= f32::EPSILON {
        return from;
    }
    spot(fx + dx / length * distance, fy + dy / length * distance)
}

/// The spot a given distance from one, square to the way it looks at another.
///
/// A positive distance goes to the left of that line, a negative one to the
/// right.
pub fn aside(from: Vec2, facing: Vec2, distance: f32) -> Vec2 {
    let (fx, fy) = xy(from);
    let (tx, ty) = xy(facing);
    let (dx, dy) = (tx - fx, ty - fy);
    let length = (dx * dx + dy * dy).sqrt();
    if length <= f32::EPSILON {
        return spot(fx + distance, fy);
    }
    spot(fx - dy / length * distance, fy + dx / length * distance)
}

/// Where along a segment a spot falls, and how far off the line it is.
///
/// The first number runs from zero at one end to one at the other, and never
/// leaves that span however far past an end the spot lies.
pub fn onto(one: Vec2, other: Vec2, at: Vec2) -> (f32, f32) {
    let (ax, ay) = xy(one);
    let (bx, by) = xy(other);
    let (px, py) = xy(at);
    let (dx, dy) = (bx - ax, by - ay);
    let length = dx * dx + dy * dy;
    if length <= f32::EPSILON {
        return (0.0, span(one, at));
    }
    let part = (((px - ax) * dx + (py - ay) * dy) / length).clamp(0.0, 1.0);
    let (nx, ny) = (ax + dx * part, ay + dy * part);
    (part, ((px - nx) * (px - nx) + (py - ny) * (py - ny)).sqrt())
}

/// The spot a part of the way along a segment.
pub fn between(one: Vec2, other: Vec2, part: f32) -> Vec2 {
    let (ax, ay) = xy(one);
    let (bx, by) = xy(other);
    spot(ax + (bx - ax) * part, ay + (by - ay) * part)
}

/// The facing from one spot towards another, in the brads the wire counts in.
pub fn facing_at(from: Vec2, to: Vec2) -> f32 {
    let (fx, fy) = xy(from);
    let (tx, ty) = xy(to);
    let turns = (ty - fy).atan2(tx - fx) / std::f32::consts::TAU;
    (turns.rem_euclid(1.0)) * 65536.0
}

/// How far one facing is from another, as a part of a half turn.
///
/// Zero looking straight at it, one looking straight away.
pub fn facing_off(facing: Angle, wanted: f32) -> f32 {
    let gap = (wanted - f32::from(facing.brads)).rem_euclid(65536.0);
    let gap = if gap > 32768.0 { 65536.0 - gap } else { gap };
    gap / 32768.0
}
