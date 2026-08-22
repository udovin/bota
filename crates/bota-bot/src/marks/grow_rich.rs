//! Be worth as much as possible.
//!
//! Net worth, not the purse: unspent gold plus what everything it owns cost,
//! counting the bag, the stash and whatever the courier is carrying. Spending
//! moves gold from one side of that to the other and leaves it unchanged, so
//! this lesson has nothing to say about what is bought, only about how much
//! there is.
//!
//! Paid a tick at a time as the difference since the tick before, which adds up
//! over a match to what it ended up worth less what it started with. Downwards
//! as well: gold lost on dying is net worth lost, and a lesson that only
//! counted the gains would be paid the same for a rich hero and a reckless one.
//!
//! One mark a gold, so the number this lesson reports is net worth itself. It
//! is an order of magnitude above every other lesson's and that is no trouble:
//! a lesson's marks are its own and are never added to another's.

use crate::{Carried, Field, Moment};

/// What one gold of net worth is worth.
const A_GOLD: f32 = 1.0;

/// What this tick was worth to this lesson.
pub fn score(now: &Moment, carried: &mut Carried) -> f32 {
    let worth = networth(now.field);
    let was = carried.was_owned.replace(worth);
    let Some(was) = was else {
        return 0.0;
    };
    A_GOLD * (worth - was) as f32
}

/// What the seat is worth: the gold it holds and the goods it owns.
///
/// Only the items the shop knows a price for. One it does not is left out
/// rather than guessed at, which undercounts by however much such an item cost.
fn networth(field: &Field) -> i32 {
    field.seat.gold.unwrap_or(0) + crate::worth_of_goods(field)
}
