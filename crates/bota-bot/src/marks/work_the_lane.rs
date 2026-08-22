//! The same as meeting the wave, over the early game.
//!
//! The same marks against a clock four times as long. What the longer match
//! asks for is not a different habit but the same one held for longer, so it is
//! scored by the same formula rather than by a copy of it that could drift.

use crate::{Carried, Moment};

/// What this tick was worth to this lesson.
pub fn score(now: &Moment, carried: &mut Carried) -> f32 {
    super::meet_the_wave::score(now, carried)
}
