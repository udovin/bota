//! The seam between the game and whatever is choosing.
//!
//! Everything on the game's side of it ends at [`Shown`]: numbers and a flag
//! per deed. Everything on the other side begins there and answers with one
//! number. A mind that knows what a creep is has reached across the seam, and
//! a game that knows what a weight is has reached back.
//!
//! Two are here. [`Nothing`] stands still and is what the wire is tested with
//! when no model is wanted; [`FirstAllowed`] takes the first thing it may do,
//! which is a poor bot and a useful floor — anything that cannot beat it is
//! not working.

use crate::Shown;

/// Whatever chooses a deed.
pub trait Mind {
    /// Which deed, by its number. Nothing means no order this tick.
    ///
    /// Answering with a number whose flag is false is a mistake on the mind's
    /// part; the loop drops it and the tick is wasted, which is why the model
    /// masks rather than being trusted.
    fn choose(&mut self, shown: &Shown) -> Option<usize>;

    /// Told when a match begins, for anything that keeps a memory.
    fn starting(&mut self) {}

    /// Told what a tick paid, whether it decided anything on that tick or not.
    ///
    /// Every tick, because what a decision led to happens over the ticks after
    /// it and most of those decide nothing new. A mind that is only playing
    /// has no use for this and ignores it.
    fn paid(&mut self, _at: u32, _marks: f32) {}
}

/// A mind that does nothing at all.
pub struct Nothing;

impl Mind for Nothing {
    fn choose(&mut self, _shown: &Shown) -> Option<usize> {
        None
    }
}

/// A mind that takes the first deed it is allowed.
///
/// The floor to measure against. It stands still for ever, because standing is
/// the first deed in the list — which is exactly the point: a model that has
/// learned nothing should be no better.
pub struct FirstAllowed;

impl Mind for FirstAllowed {
    fn choose(&mut self, shown: &Shown) -> Option<usize> {
        shown.allowed.iter().position(|may| *may)
    }
}
