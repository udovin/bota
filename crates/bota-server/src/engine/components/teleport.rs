//! A teleport being channelled.

use bota_proto::Vec2;

/// What one entity is in the middle of teleporting to.
///
/// It stands still for as long as this runs, and any order at all takes it
/// away. The scroll paying for it is spent when it carries, not before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Teleport {
    /// Ticks before it carries.
    pub ticks_left: u32,
    /// Where it carries to.
    pub to: Vec2,
    /// Which slot of the bag holds the scroll paying for it.
    pub slot: usize,
}
