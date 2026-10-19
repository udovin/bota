//! What a participant and a game server say to each other while one match
//! runs, and what a replay keeps of it.

use serde::{Deserialize, Serialize};

use crate::{
    EventKind, HeroId, MatchInfo, MatchStats, RejectReason, SlotId, Team, Ticket, UnitOrder,
    WorldView,
};

/// Anything a participant can say to a game server.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ToGame {
    /// First message on a connection, before anything else is accepted:
    /// the ticket the hub gave in a [`FromHub::Play`](crate::FromHub::Play).
    Join {
        /// What lets this connection in.
        ticket: Ticket,
    },
    /// Show the seat's own side which hero a seat this connection holds is
    /// set on, while heroes are picked; the other side is not told. Nothing
    /// is fixed by it, and a later one replaces it. A refused one comes back
    /// in a [`FromGame::PickRefused`].
    Select {
        /// Which seat.
        seat: SlotId,
        /// Which hero.
        hero: HeroId,
    },
    /// Fix the hero of a seat this connection holds, while heroes are
    /// picked, for everybody to see. A pick is final; a refused one comes
    /// back in a [`FromGame::PickRefused`].
    Pick {
        /// Which seat.
        seat: SlotId,
        /// Which hero.
        hero: HeroId,
    },
    /// Tell units what to do, each order naming its own.
    ///
    /// At most one order per unit survives per tick and the last one wins,
    /// so re-sending is harmless. Orders apply on the next tick in seat and
    /// unit order, whatever order they arrived in.
    Orders {
        /// At most [`MAX_ORDERS_PER_MESSAGE`](crate::MAX_ORDERS_PER_MESSAGE)
        /// of them. A refused one comes back whole in a
        /// [`FromGame::OrderRejected`].
        orders: Vec<UnitOrder>,
    },
    /// Declare that this participant has finished thinking about a tick, for
    /// every seat it holds.
    ///
    /// Only meaningful in [`TickMode::Lockstep`](crate::TickMode::Lockstep).
    Ack {
        /// The tick being acknowledged.
        tick: u32,
    },
    /// Choose whose eyes a spectator watches through.
    ///
    /// A seat sets the fog of the snapshots to its side's and brings that
    /// seat's own orders; absent watches everything and is told none.
    /// Ignored for a seated participant, whose eyes are its own.
    ViewAs {
        /// Which seat to watch as. Absent for the whole map.
        seat: Option<SlotId>,
    },
    /// Say a line in the match. A line starting with `/` is a command the
    /// game server runs instead.
    Chat {
        /// What is said, at most [`MAX_CHAT_CHARS`](crate::MAX_CHAT_CHARS)
        /// characters.
        text: String,
        /// Said to the speaker's own side alone. Not read for a command.
        team: bool,
    },
}

/// Why a game server turned a connection away.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GameReason {
    /// The ticket lets nobody into a match this server runs, or the match
    /// has ended.
    UnknownTicket,
    /// The first message was not a [`ToGame::Join`], or a second one came.
    NotJoined,
}

/// Why a game server turned a selection or a pick down.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PickReason {
    /// The seat is not one this connection holds.
    NotYourSeat,
    /// The seat has its hero already: a pick is final.
    AlreadyPicked,
    /// No hero has this id.
    NoSuchHero,
    /// Heroes are not being picked any more.
    NotPicking,
}

/// One seat while heroes are picked.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PickSeat {
    /// Which seat.
    pub slot: SlotId,
    /// Which side it plays for.
    pub team: Team,
    /// The account holding it.
    pub login: String,
    /// The hero it picked. Absent until it picks.
    pub hero: Option<HeroId>,
    /// The hero it is set on and has not picked yet. Told to its own side
    /// and to spectators only; absent for the other side, while it is set
    /// on none, and once it has picked.
    pub selected: Option<HeroId>,
}

/// Where the picking of heroes stands, as one connection sees it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PickState {
    /// Every seat, sorted by [`SlotId`].
    pub seats: Vec<PickSeat>,
    /// The seats the receiver holds, in slot order. Empty for a spectator.
    pub yours: Vec<SlotId>,
    /// Milliseconds left, as of sending, before every seat without a hero
    /// is given the one it is set on, or one at random, and the match
    /// begins.
    pub ms_left: u32,
}

/// Anything a game server can say to a participant.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum FromGame {
    /// Heroes are being picked. Sent on joining while they are, and to
    /// everybody whenever a seat selects or picks; a
    /// [`FromGame::MatchStart`] follows once every seat has its hero.
    Picking {
        /// Where the picking stands, as the receiver may know it.
        state: PickState,
    },
    /// A selection or a pick was not accepted.
    PickRefused {
        /// The seat it was for.
        seat: SlotId,
        /// The hero asked for.
        hero: HeroId,
        /// Why.
        reason: PickReason,
    },
    /// The match has begun, or the connection has joined it: the public
    /// description of it.
    MatchStart {
        /// The public description of it.
        info: MatchInfo,
    },
    /// The state of the world on one tick, already filtered through this team's
    /// fog.
    ///
    /// Sent whole on every tick. A client can start rendering from any one of
    /// these without having received an earlier one.
    Snapshot {
        /// The state. Its own [`WorldView::tick`] says which tick it is.
        view: WorldView,
    },
    /// What happened during a tick, filtered to what this team may know.
    /// Sent after every snapshot, including an empty batch, to complete that tick.
    Events {
        /// Which tick these belong to.
        tick: u32,
        /// The events, in the order the simulation produced them.
        events: Vec<EventKind>,
    },
    /// An order was not accepted.
    OrderRejected {
        /// The order as it was given: which unit, and what.
        order: UnitOrder,
        /// Why.
        reason: RejectReason,
    },
    /// The orders accepted on one tick, as far as the receiver may know
    /// them.
    ///
    /// Sent to a spectator watching through one seat's eyes, and carries
    /// that seat's orders alone. Watching everything brings none, and a
    /// seated participant knows its own.
    Orders {
        /// Which tick they were applied on.
        tick: u32,
        /// The orders, in the order they were applied.
        orders: Vec<UnitOrder>,
    },
    /// The match is over.
    MatchOver {
        /// Which side won; [`Team::Neutral`] for a draw.
        winner: Team,
        /// Final numbers.
        stats: MatchStats,
    },
    /// A participant's connection to the match ended.
    ParticipantLeft {
        /// Which seats it held. The seats stay in the match and their heroes
        /// keep standing there.
        slots: Vec<SlotId>,
    },
    /// Somebody in the match said a line. The speaker hears its own line
    /// too.
    Chat {
        /// Who said it: an account's login.
        from: String,
        /// The side the speaker plays for. Absent for a spectator.
        side: Option<Team>,
        /// Said to that side alone.
        team: bool,
        /// What was said.
        text: String,
    },
    /// A line from the game server itself: the answer to a command, or word
    /// to the whole match that one was run.
    Notice {
        /// What it says.
        text: String,
    },
    /// The connection was turned away, and is closed after this.
    Refused {
        /// Why.
        reason: GameReason,
    },
}

/// One frame of a replay file.
///
/// A replay file is a sequence of length-prefixed frames, framed exactly like
/// the socket, each carrying one record. Read by the client in replay mode.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ReplayRecord {
    /// A message of the fogless spectator stream.
    Msg(FromGame),
    /// The orders the server accepted on one tick.
    Orders {
        /// Which tick they were applied on.
        tick: u32,
        /// At most one order per unit, in the order they were applied.
        orders: Vec<UnitOrder>,
    },
}
