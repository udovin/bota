//! Turning a lesson into its own function, and keeping the running total.
//!
//! [`score`] is the one place in the crate that branches on which lesson is
//! being taught. Everything below it is a lesson's own file and knows about no
//! other lesson.
//!
//! Because a lesson's marks depend on nothing an earlier lesson taught, every
//! number can be read off one match: each lesson counts the ticks inside its
//! own window and stops. So a match run to the longest lesson's clock scores
//! the whole ladder, and what comes back describes one game rather than seven.
//!
//! Marks are not the score of a match. A lesson is a ladder to be kicked away:
//! what is paid for is a habit worth having early, not what winning is. A bot
//! that has learned the first lesson perfectly walks beautifully to its lane and
//! farms nothing.

use super::{
    find_the_lane, grow_rich, hold_the_lane, meet_the_wave, stock_up, take_the_towers,
    work_the_lane,
};
use crate::{Card, Carried, LADDER, LESSONS, Lesson, Moment};

/// What one tick was worth to one lesson.
///
/// The one branch on which lesson is which. A lesson's own function is handed
/// the tick and what that lesson remembers, and answers with one number.
pub fn score(lesson: Lesson, now: &Moment, carried: &mut Carried) -> f32 {
    match lesson {
        Lesson::StockUp => stock_up::score(now, carried),
        Lesson::FindTheLane => find_the_lane::score(now, carried),
        Lesson::HoldTheLane => hold_the_lane::score(now, carried),
        Lesson::MeetTheWave => meet_the_wave::score(now, carried),
        Lesson::WorkTheLane => work_the_lane::score(now, carried),
        Lesson::TakeTheTowers => take_the_towers::score(now, carried),
        Lesson::GrowRich => grow_rich::score(now, carried),
    }
}

/// The running marks of every lesson over one match.
#[derive(Clone, Copy, Debug, Default)]
pub struct Marker {
    card: Card,
    carried: [Carried; LESSONS],
}

impl Marker {
    /// A marker with nothing counted yet.
    pub fn new() -> Marker {
        Marker::default()
    }

    /// What every lesson has paid so far.
    pub fn card(&self) -> Card {
        self.card
    }

    /// Scores one whole tick, and says what it paid lesson by lesson.
    ///
    /// Called once a tick with everything that happened during it, so that a
    /// lesson is one function rather than one for standing and another for
    /// blows.
    pub fn tick(&mut self, now: &Moment) -> Card {
        let mut paid = Card::new();
        for rung in &LADDER {
            let at = rung.lesson.at();
            if rung.lesson.covers(now.tick()) {
                paid.marks[at] = score(rung.lesson, now, &mut self.carried[at]);
            }
        }
        self.card.add(&paid);
        paid
    }
}
