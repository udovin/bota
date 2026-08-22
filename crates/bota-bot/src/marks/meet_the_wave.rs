//! Be where their wave is, and hit it.
//!
//! Standing near their nearest creep, walking towards it, and the blows that
//! land. A creep taken is worth ten of a blow that only lands, so that a lesson
//! which pays for hitting does not end up teaching a bot to hit everything and
//! take nothing.
//!
//! Blows are counted against the other side only. Counted for the swing alone,
//! this lesson is answered by cutting down its own wave, which is always beside
//! it and never fights back — and the gradient trainer found exactly that and
//! played a whole match on it.

use super::common;
use crate::{Carried, Moment};

/// What one tick of standing with their wave is worth.
const A_TICK_IN_PLACE: f32 = 0.01;
/// What closing one unit of the distance to it is worth.
const A_STEP_HOME: f32 = 1e-3;
/// What a blow that lands is worth.
const A_BLOW: f32 = 1.0;
/// What a creep taken is worth. Ten blows, as asked.
const A_CREEP: f32 = 10.0;

/// What this tick was worth to this lesson.
pub fn score(now: &Moment, carried: &mut Carried) -> f32 {
    let mut marks = A_BLOW * common::blows_landed(now) as f32 + A_CREEP * f32::from(now.took);
    if !now.alive() {
        return marks;
    }
    if let Some(creep) = now.field.creeps.first() {
        let off = crate::span(now.at(), creep.pos);
        let was = carried.was_off.replace(off);
        marks += A_TICK_IN_PLACE * common::nearness(off) + A_STEP_HOME * common::closed(was, off);
    }
    marks
}
