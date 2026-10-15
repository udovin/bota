//! The messages themselves, and the match types they carry.

use crate::{
    AccountId, AccountInfo, Auth, EntityId, EventKind, HallReason, HeroId, ItemId, MapId,
    MatchFilter, MatchId, MatchState, MatchSummary, ModeId, ModeInfo, Order, PlayerId, Role,
    SlotId, Team, Token, TournamentFormat, TournamentId, TournamentInfo, TournamentSummary, Vec2,
    WorldView,
};
use serde::{Deserialize, Serialize};

/// How the server decides when to advance a tick.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TickMode {
    /// Advance on a wall clock at the configured rate. An order that misses its
    /// tick applies on the next one.
    Realtime,
    /// Advance only once every participant has acknowledged the tick, with no
    /// bound on how long one may take.
    Lockstep,
}

/// One seat's hero choice.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pick {
    /// Which seat.
    pub slot: SlotId,
    /// Which side it plays for.
    pub team: Team,
    /// Which hero it picked.
    pub hero: HeroId,
}

/// What the shop asks for one item.
///
/// The price of a thing nobody holds yet, so it belongs to no unit and is
/// stated once for the whole match. Everything an item is worth to whoever
/// carries it rides in [`ItemView`] instead, where it can differ between two
/// units holding the same item.
///
/// [`ItemView`]: crate::ItemView
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ShopEntry {
    /// Which item.
    pub id: ItemId,
    /// What it costs whole, in gold.
    pub cost: i32,
    /// What it is built from. Empty for one bought whole.
    pub components: Vec<ItemId>,
}

/// The public description of a match.
///
/// Sent when the match begins and to anyone joining later.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MatchInfo {
    /// Identifies this match in the hall, the logs and the replay files.
    pub match_id: MatchId,
    /// Which map is being played.
    pub map: MapId,
    /// Simulation ticks per second.
    pub tick_rate: u16,
    /// Ticks before the game clock reaches zero. The clock counts up from
    /// minus this; creep waves start at zero.
    pub pregame_ticks: u32,
    /// Every tree on the map. Static for the whole match; sent once here so
    /// the client can draw and never has to know the layout rules.
    pub trees: Vec<Vec2>,
    /// Cells per terrain axis.
    pub terrain_cells: u32,
    /// Run-length encoded terrain cells, row-major from the south-west
    /// corner, one byte each: bit 7 walkable ground, bit 6 river water, the
    /// low bits the elevation tier.
    pub terrain_rle: Vec<(u16, u8)>,
    /// Cells that block sight lines regardless of elevation: trees and the
    /// map's fog blocker walls. The client shades its own fog with these.
    pub opaque_cells: Vec<(u16, u16)>,
    /// How the server advances ticks.
    pub mode: TickMode,
    /// Every seat and its hero, sorted by [`SlotId`].
    pub picks: Vec<Pick>,
    /// Everything the shop sells, in item id order.
    pub shop: Vec<ShopEntry>,
}

/// Why the server refused an order.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RejectReason {
    /// The order arrived for a seat this connection does not hold.
    NotYourSlot,
    /// The hero is dead.
    HeroDead,
    /// The target does not exist, or is not currently visible to this team.
    /// One answer covers both cases.
    UnknownTarget,
    /// The ability or item does not accept this kind of target.
    WrongTargetKind,
    /// The ability or item is still on cooldown.
    OnCooldown,
    /// The target is beyond the ability's cast range.
    OutOfRange,
    /// Not enough mana.
    NotEnoughMana,
    /// Not enough gold.
    NotEnoughGold,
    /// The referenced ability or inventory slot is empty.
    EmptySlot,
    /// The ability works on its own and is never cast.
    NotCastable,
    /// The ability has no points in it yet.
    NotLearned,
    /// The item has no charges left to spend.
    NoCharges,
    /// The item has come out of the backpack and is not working yet.
    NotReady,
    /// The order named a unit this seat does not drive.
    NotYourUnit,
    /// No item with this id is sold.
    UnknownItem,
    /// No skill point is available, or the ability is already at its cap.
    CannotLevelUp,
    /// Buying and selling require standing in the fountain area.
    NotAtShop,
    /// The inventory is full.
    InventoryFull,
    /// A status such as silence or stun forbids this action right now.
    Disabled,
    /// The match has not started or has already finished.
    NotPlaying,
    /// Only the seat that bought an item may sell it or mark it for sale.
    NotYourItem,
    /// The spot aimed at is ground nothing may stand on.
    ClosedGround,
    /// The slot named is not one the unit carries on itself.
    NotInBag,
    /// The match was not started with cheats on.
    NoCheats,
}

/// Final numbers for one seat.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotStats {
    /// Which seat.
    pub slot: SlotId,
    /// Kills scored.
    pub kills: u16,
    /// Times died.
    pub deaths: u16,
    /// Kills assisted.
    pub assists: u16,
    /// Enemy creeps last hit.
    pub last_hits: u16,
    /// Friendly creeps denied.
    pub denies: u16,
    /// Gold earned over the whole match, spent or not.
    pub net_worth: i32,
    /// Damage dealt to enemy heroes.
    pub hero_damage: i32,
    /// Damage dealt to enemy buildings.
    pub structure_damage: i32,
}

/// Everything a match produced, sent once when it ends.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MatchStats {
    /// Length of the match in ticks.
    pub duration: u32,
    /// One entry per seat, sorted by [`SlotId`].
    pub slots: Vec<SlotStats>,
}

/// One accepted order, as it is told to whoever may know of it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SlotOrder {
    /// Which seat gave it.
    pub slot: SlotId,
    /// Which unit it was for. Absent means the seat's own hero.
    pub unit: Option<EntityId>,
    /// The order itself.
    pub order: Order,
}

/// Anything a participant can say to the server.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ClientMsg {
    /// First message on a connection, before anything else is accepted.
    Hello {
        /// Who this connection is.
        auth: Auth,
    },
    /// Choose a hero for the seat this connection holds.
    PickHero {
        /// Which hero.
        hero: HeroId,
    },
    /// Declare readiness, or withdraw it. The match starts when every seat is
    /// held, picked and ready.
    SetReady(bool),
    /// Tell one of the units this seat drives what to do.
    ///
    /// At most one order per seat survives per tick and the last one wins, so
    /// re-sending is harmless.
    Order {
        /// Sequence number, unique per connection and increasing. A
        /// [`ServerMsg::OrderRejected`] names the order by this.
        seq: u32,
        /// Which unit it is for. Absent means the seat's own hero, which is
        /// what most orders are for.
        unit: Option<EntityId>,
        /// What to do.
        order: Order,
    },
    /// Declare that this participant has finished thinking about a tick.
    ///
    /// Only meaningful in [`TickMode::Lockstep`].
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
    /// Replace the account's password.
    ChangePassword {
        /// The password as it is.
        old: String,
        /// The password as it will be.
        new: String,
    },
    /// Forget the token this connection signed in with, or was given.
    Logout,
    /// Ask for a page of the archive, newest first.
    ListMatches {
        /// Which matches.
        filter: MatchFilter,
        /// Only matches with a smaller id. Absent starts from the newest.
        before: Option<MatchId>,
        /// At most this many.
        limit: u16,
    },
    /// Open a match and take its first seat. Leaves whatever match the
    /// connection was in.
    CreateMatch {
        /// Which mode.
        mode: ModeId,
        /// What the hall will call it.
        name: String,
        /// The seed of its randomness. Absent draws a fresh one.
        seed: Option<u64>,
    },
    /// Take a seat in an open match of this mode, opening one when none has
    /// a seat free.
    QuickPlay {
        /// Which mode.
        mode: ModeId,
    },
    /// Enter a match. Leaves whatever match the connection was in.
    JoinMatch {
        /// Which match.
        id: MatchId,
        /// With a seat or without.
        role: Role,
    },
    /// Leave the match the connection is in, back to the hall. A seat in a
    /// running match stays this account's.
    LeaveMatch,
    /// Move to another seat of a gathering match, or give the seat up and
    /// watch.
    TakeSeat {
        /// Which seat. Absent gives the seat up.
        slot: Option<SlotId>,
    },
    /// Throw an account out of a gathering match. Host only.
    Kick {
        /// Which account.
        account: AccountId,
    },
    /// Open a tournament for registration.
    CreateTournament {
        /// What it is called.
        name: String,
        /// The mode its games are played under.
        mode: ModeId,
        /// How teams are paired.
        format: TournamentFormat,
        /// Games in a series: one, three, five or seven.
        best_of: u8,
    },
    /// Register in a tournament, in a team by name. A new name makes the
    /// team, a known name joins it.
    JoinTournament {
        /// Which tournament.
        id: TournamentId,
        /// Which team.
        team: String,
    },
    /// Withdraw from the tournament this account is registered in, while it
    /// still registers.
    LeaveTournament,
    /// Close registration and pair the teams. Organizer only.
    StartTournament {
        /// Which tournament.
        id: TournamentId,
    },
    /// Take a tournament down before it starts. Organizer only.
    CancelTournament {
        /// Which tournament.
        id: TournamentId,
    },
    /// Receive the state of a tournament whenever it changes. Absent stops.
    ViewTournament {
        /// Which tournament.
        id: Option<TournamentId>,
    },
    /// Ask for the replay of a match, as a run of [`ServerMsg::ReplayChunk`]
    /// ending in a [`ServerMsg::ReplayEnd`].
    FetchReplay {
        /// Which match.
        match_id: MatchId,
    },
}

/// Anything the server can say to a participant.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ServerMsg {
    /// Accepts a [`ClientMsg::Hello`].
    Welcome {
        /// Handle for this connection.
        player_id: PlayerId,
        /// The account signed in. Absent for a guest.
        account: Option<AccountInfo>,
        /// A key that signs the same account in again. Absent for a guest.
        token: Option<Token>,
        /// Every mode the server offers, in id order.
        modes: Vec<ModeInfo>,
    },
    /// The match the connection is in, resent whenever its composition or
    /// stage changes. Receiving it means being in that match.
    MatchState {
        /// The match.
        state: MatchState,
    },
    /// The match has begun.
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
        /// Which order, by the sequence number it was sent with.
        seq: u32,
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
        /// The orders, sorted by [`SlotId`].
        orders: Vec<SlotOrder>,
    },
    /// The match is over.
    MatchOver {
        /// Which side won.
        winner: Team,
        /// Final numbers.
        stats: MatchStats,
    },
    /// A participant's connection ended.
    ParticipantLeft {
        /// Which connection.
        player_id: PlayerId,
        /// Which seat it held. Absent for a spectator. The seat stays in the
        /// match and its hero keeps standing there.
        slot: Option<SlotId>,
    },
    /// Something said outside a running match was not accepted.
    Refused {
        /// Why.
        reason: HallReason,
    },
    /// What the hall holds, sent on entering it and whenever it changes.
    /// Receiving it means being in the hall.
    Hall {
        /// Every match gathering, playing or lately ended, newest first.
        matches: Vec<MatchSummary>,
        /// Every tournament not yet finished, and the lately finished ones.
        tournaments: Vec<TournamentSummary>,
    },
    /// A page of the archive, answering a [`ClientMsg::ListMatches`].
    Matches {
        /// The matches, newest first.
        page: Vec<MatchSummary>,
        /// Where the next page starts. Absent when this was the last.
        before: Option<MatchId>,
    },
    /// A tournament whole, sent to its members and to whoever views it,
    /// whenever it changes.
    TournamentState {
        /// The tournament.
        state: TournamentInfo,
    },
    /// A piece of a replay file, in file order.
    ReplayChunk {
        /// Which match.
        match_id: MatchId,
        /// Where in the file this piece starts.
        offset: u64,
        /// The bytes.
        bytes: Vec<u8>,
    },
    /// The replay file has been sent whole.
    ReplayEnd {
        /// Which match.
        match_id: MatchId,
        /// The size of the file.
        total: u64,
    },
}

/// One frame of a replay file.
///
/// A replay file is a sequence of length-prefixed frames, framed exactly like
/// the socket, each carrying one record. Read by the client in replay mode.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ReplayRecord {
    /// A message of the fogless spectator stream.
    Msg(ServerMsg),
    /// The orders the server accepted on one tick.
    Orders {
        /// Which tick they were applied on.
        tick: u32,
        /// At most one order per seat, sorted by [`SlotId`].
        orders: Vec<SlotOrder>,
    },
}
