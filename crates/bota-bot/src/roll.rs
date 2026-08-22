//! What a match looked like to the model, kept so it can be learned from.
//!
//! One frame a decision: the numbers it was shown, which deeds it was allowed,
//! and which it took. Alongside them runs what every tick paid, whether it
//! decided anything on that tick or not — that is what lets a decision be
//! credited with what followed it rather than with how the whole match ended.
//!
//! The numbers kept are the ones actually fed, history and all. Keeping the
//! tick alone and stacking it again at training time would mean two pieces of
//! code agreeing about how a stack is built, and they would not agree for
//! long.

use crate::{DEEDS, Dice, INPUT, Mind, Shown};

/// One decision.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// What the model was fed, history and all.
    pub numbers: Vec<f32>,
    /// Which deeds it was allowed.
    pub allowed: Vec<bool>,
    /// Which it took.
    pub chosen: usize,
    /// Which tick it was taken on.
    pub at: u32,
    /// What followed it, once the match is over and that is known.
    pub worth: f32,
}

/// Every decision of one seat's match, and what the match paid tick by tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Roll {
    /// The frames, in the order they happened.
    pub frames: Vec<Frame>,
    /// What the seat was paid on each tick, from the first it saw.
    pub paid: Vec<f32>,
    /// The tick the first of those belongs to.
    pub from: u32,
}

impl Roll {
    /// Nothing recorded yet.
    pub fn new() -> Roll {
        Roll::default()
    }

    /// Notes what a tick paid, whether anything was decided on it or not.
    pub fn paid_on(&mut self, tick: u32, paid: f32) {
        if self.paid.is_empty() {
            self.from = tick;
        }
        let at = tick.saturating_sub(self.from) as usize;
        if at >= self.paid.len() {
            self.paid.resize(at + 1, 0.0);
        }
        self.paid[at] += paid;
    }

    /// Credits every decision with what followed it.
    ///
    /// What followed, and less and less of it the further off it is. A match
    /// is thousands of decisions and one number at the end, so crediting them
    /// all with that number tells a good decision in a bad match that it was
    /// bad. The window is what keeps a decision from being held to the rest of
    /// the match: past it, it is somebody else's doing.
    pub fn settle(&mut self, discount: f32, window: usize) {
        let mut running = vec![0.0; self.paid.len() + 1];
        for at in (0..self.paid.len()).rev() {
            running[at] = self.paid[at] + discount * running[at + 1];
        }
        let ends = discount.powi(window as i32);
        for frame in &mut self.frames {
            let at = frame.at.saturating_sub(self.from) as usize;
            let now = running.get(at).copied().unwrap_or(0.0);
            let later = running.get(at + window).copied().unwrap_or(0.0);
            frame.worth = now - ends * later;
        }
    }

    /// Everything the match paid, added up.
    pub fn paid_in_all(&self) -> f32 {
        self.paid.iter().sum()
    }

    /// How many decisions are held.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether none are.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Keeps it down to a size worth going over, taken at random.
    pub fn thin_to(&mut self, most: usize, dice: &mut Dice) {
        if self.frames.len() <= most || most == 0 {
            return;
        }
        let mut order: Vec<usize> = (0..self.frames.len()).collect();
        for at in (1..order.len()).rev() {
            order.swap(at, (dice.next_u64() % (at as u64 + 1)) as usize);
        }
        order.truncate(most);
        order.sort_unstable();
        self.frames = order
            .into_iter()
            .map(|at| self.frames[at].clone())
            .collect();
    }
}

/// A mind that plays and writes down what it did.
///
/// It is the mind itself rather than something watching one, because what has
/// to be written down is the numbers as fed — history and all — and only the
/// mind knows those.
pub struct Student<M> {
    /// The mind doing the choosing.
    pub mind: M,
    /// What it saw and did.
    pub roll: Roll,
}

impl<M: Mind> Student<M> {
    /// A mind with a roll beside it.
    pub fn new(mind: M) -> Student<M> {
        Student {
            mind,
            roll: Roll::new(),
        }
    }
}

impl<M: Mind + Fed> Mind for Student<M> {
    fn choose(&mut self, shown: &Shown) -> Option<usize> {
        let chosen = self.mind.choose(shown)?;
        let numbers = self.mind.what_was_fed();
        debug_assert_eq!(numbers.len(), INPUT, "the model was fed the wrong length");
        debug_assert_eq!(shown.allowed.len(), DEEDS, "a flag per deed");
        self.roll.frames.push(Frame {
            numbers,
            allowed: shown.allowed.clone(),
            chosen,
            at: shown.at,
            worth: 0.0,
        });
        Some(chosen)
    }

    fn starting(&mut self) {
        self.mind.starting();
    }

    fn paid(&mut self, at: u32, marks: f32) {
        self.roll.paid_on(at, marks);
        self.mind.paid(at, marks);
    }
}

/// A mind that can say what it last put in front of its model.
pub trait Fed {
    /// The numbers it was fed for the choice it just made.
    fn what_was_fed(&self) -> Vec<f32>;
}
