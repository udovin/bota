//! What every lesson has paid, side by side.

use crate::{LADDER, LESSONS, Lesson};

/// One number a lesson, in ladder order.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Card {
    /// What each lesson paid, indexed by where it sits on the ladder.
    pub marks: [f32; LESSONS],
}

impl Card {
    /// Nothing paid yet.
    pub fn new() -> Card {
        Card::default()
    }

    /// What one lesson paid.
    pub fn of(&self, lesson: Lesson) -> f32 {
        self.marks[lesson.at()]
    }

    /// Adds another card's marks onto this one.
    pub fn add(&mut self, other: &Card) {
        for (mine, theirs) in self.marks.iter_mut().zip(other.marks) {
            *mine += theirs;
        }
    }

    /// The same, with every mark divided by a count.
    pub fn over(&self, many: usize) -> Card {
        let many = many.max(1) as f32;
        let mut out = Card::new();
        for (into, from) in out.marks.iter_mut().zip(self.marks) {
            *into = from / many;
        }
        out
    }

    /// One line a lesson, for printing.
    pub fn lines(&self) -> Vec<String> {
        LADDER
            .iter()
            .map(|rung| format!("{:>16}: {:.1}", rung.name, self.of(rung.lesson)))
            .collect()
    }
}
