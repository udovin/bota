//! How long something stands before it goes on its own.

/// The time an entity has left in the world.
///
/// What carries it is taken out when this runs out, whether anything killed it
/// or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Expiry {
    /// Ticks before it goes.
    pub ticks_left: u32,
}
