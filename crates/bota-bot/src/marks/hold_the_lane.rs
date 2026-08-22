//! Stand on the line and up with the far end of its own wave.
//!
//! Two halves. The line the lane runs along, and the creep of its own that has
//! come furthest up it — which is where a lane is held from. The line alone
//! would be answered by standing in its own fountain, which sits on it.
//!
//! Until the first wave walks out there is no wave to stand with, and for three
//! ticks in four of this lesson there is not. What it wants then is the spot the
//! waves will meet at, which is what the lesson before it wanted.

use super::common;
use crate::{Carried, Moment};

/// What one tick of standing where it belongs is worth.
const A_TICK_IN_PLACE: f32 = 0.01;
/// What closing one unit of the distance to its wave is worth.
const A_STEP_HOME: f32 = 1e-3;

/// What this tick was worth to this lesson.
pub fn score(now: &Moment, carried: &mut Carried) -> f32 {
    let Some(lane) = now.lane else {
        return 0.0;
    };
    if !now.alive() {
        return 0.0;
    }
    let at = now.at();
    let spot = common::furthest_own_creep(now.field, lane)
        .map_or_else(|| lane.where_they_meet(), |creep| creep.pos);
    let off = crate::span(at, spot);
    let was = carried.was_off.replace(off);
    let on_the_line = common::nearness(lane.off_the_line(at));
    let with_the_wave = common::nearness(off);
    A_TICK_IN_PLACE * (on_the_line + with_the_wave) / 2.0 + A_STEP_HOME * common::closed(was, off)
}
