//! Spend the gold it starts with.
//!
//! Marks for what the goods cost, which is the gold that left. What is bought
//! is the shopping list's business; that any of it is bought at all is the
//! lesson.
//!
//! Read off what the seat owns — the bag, the stash and the courier's load,
//! each item at its price — rather than off the gold falling, which also falls
//! on death and rises on its own. Only increases count: selling gold back is
//! not spending it.

use crate::{Carried, Moment};

/// What one gold spent is worth.
///
/// Everything the shopping list can be bought for out of the gold a hero starts
/// with comes to a few marks.
const A_GOLD_SPENT: f32 = 0.01;

/// What this tick was worth to this lesson.
pub fn score(now: &Moment, carried: &mut Carried) -> f32 {
    let owned = crate::worth_of_goods(now.field);
    let was = carried.was_owned.replace(owned);
    let Some(was) = was else {
        return 0.0;
    };
    A_GOLD_SPENT * (owned - was).max(0) as f32
}
