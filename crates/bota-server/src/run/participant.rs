//! Who a running match sends to, and what its game server tells it.

use std::sync::Arc;

use bota_proto::{HeroId, PlayerId, SlotId, Team, UnitOrder};

use crate::net::Outbox;

/// Which seats a cheat is taken for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Whom {
    /// Every seat of the connection that typed it.
    Mine,
    /// One seat.
    Seat(SlotId),
    /// Every seat of one side.
    Side(Team),
    /// Every seat.
    All,
}

/// One connection a running match sends to.
pub struct Participant {
    /// Which connection.
    pub player: PlayerId,
    /// Where its frames go.
    pub outbox: Arc<Outbox>,
    /// The login its lines are signed with and its cheats told under.
    pub login: String,
    /// The seats it holds, in slot order, all on one side. Empty for a
    /// spectator.
    pub seats: Vec<SlotId>,
    /// The seat a spectator watches through. Absent watches everything.
    pub eyes: Option<SlotId>,
}

/// What a game server tells a running match.
pub enum MatchEvent {
    /// A connection entered the match.
    Join(Participant),
    /// A connection left the match, or its socket closed.
    Leave(PlayerId),
    /// A connection set one of its seats on a hero, without picking it.
    Select {
        /// Which connection.
        player: PlayerId,
        /// Which seat.
        seat: SlotId,
        /// Which hero.
        hero: HeroId,
    },
    /// A connection picked the hero of one of its seats.
    Pick {
        /// Which connection.
        player: PlayerId,
        /// Which seat.
        seat: SlotId,
        /// Which hero.
        hero: HeroId,
    },
    /// A connection gave orders to units its seats drive.
    Orders {
        /// Which connection.
        player: PlayerId,
        /// The orders, as they arrived.
        orders: Vec<UnitOrder>,
    },
    /// A connection finished thinking about a tick, for every seat it holds.
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
    /// A connection said a line, or typed a command.
    Chat {
        /// Which connection.
        player: PlayerId,
        /// What it typed.
        text: String,
        /// Said to its own side alone.
        team: bool,
    },
    /// Seats were given up for good. A side whose every seat is given up
    /// loses.
    Abandon {
        /// Which seats, in slot order.
        seats: Vec<SlotId>,
        /// The login that held them.
        login: String,
    },
    /// The match is to end without a winner.
    Stop,
}
