//! Walk out to the spot the waves will meet at, before they do.
//!
//! The spot halfway along the lane rather than anywhere on the line it runs
//! along: a fountain is on that line too, and a lesson paid for the line alone
//! is answered by never leaving it. Paid for the ground closed as well as for
//! standing there, or a policy that has never crossed a lane has nothing
//! telling it which way to set off.

use super::common;
use crate::{Carried, Moment};

/// What one tick of standing on the spot is worth.
const A_TICK_IN_PLACE: f32 = 0.01;
/// What closing one unit of the distance to it is worth.
const A_STEP_HOME: f32 = 1e-3;

/// What this tick was worth to this lesson.
pub fn score(now: &Moment, carried: &mut Carried) -> f32 {
    let Some(lane) = now.lane else {
        return 0.0;
    };
    if !now.alive() {
        return 0.0;
    }
    let off = crate::span(now.at(), lane.where_they_meet());
    let was = carried.was_off.replace(off);
    A_TICK_IN_PLACE * common::nearness(off) + A_STEP_HOME * common::closed(was, off)
}
