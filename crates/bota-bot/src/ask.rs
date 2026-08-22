//! One order, and which unit it is for.

use bota_proto::{EntityId, Order};

/// What the bot answers with on a tick.
///
/// Most orders are for the seat's own hero and name nobody; one for anything
/// else the seat drives, such as its courier, names it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ask {
    /// Whom it is for. Absent means the hero.
    pub unit: Option<EntityId>,
    /// What to do.
    pub order: Order,
}
