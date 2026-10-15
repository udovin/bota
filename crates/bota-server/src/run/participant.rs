//! Who a running match sends to, and what the hub tells it.

use std::sync::Arc;

use bota_proto::{EntityId, Order, PlayerId, SlotId};

use crate::net::Outbox;

/// One connection a running match sends to.
pub struct Participant {
    /// Which connection.
    pub player: PlayerId,
    /// Where its frames go.
    pub outbox: Arc<Outbox>,
    /// The seat it holds. Absent for a spectator.
    pub seat: Option<SlotId>,
    /// The seat a spectator watches through. Absent watches everything.
    pub eyes: Option<SlotId>,
}

/// What the hub tells a running match.
pub enum MatchEvent {
    /// A connection entered the match.
    Join(Participant),
    /// A connection left the match, or its socket closed.
    Leave(PlayerId),
    /// A seat gave an order.
    Order {
        /// Which connection.
        player: PlayerId,
        /// The order's sequence number on that connection.
        seq: u32,
        /// Which unit it is for. Absent means the seat's own hero.
        unit: Option<EntityId>,
        /// What to do.
        order: Order,
    },
    /// A seat finished thinking about a tick.
    Ack {
        /// Which connection.
        player: PlayerId,
        /// The tick acknowledged.
        tick: u32,
    },
    /// A spectator chose whose eyes to watch through.
    ViewAs {
        /// Which connection.
        player: PlayerId,
        /// Which seat. Absent for everything.
        seat: Option<SlotId>,
    },
    /// The hub wants the match ended without a winner.
    Stop,
}
