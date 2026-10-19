//! What the messages carry about a match: its terms, its orders and how it
//! ended.

use crate::{EntityId, HeroId, ItemId, MapId, MatchId, Order, Rules, SlotId, Team, Vec2};
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
    /// What the match is played to and what runs in it.
    pub rules: Rules,
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
    /// The order named a unit no seat of this connection drives, or one that
    /// is not standing.
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
    /// The message carried more than [`MAX_ORDERS_PER_MESSAGE`] orders; this
    /// one was past the limit.
    TooManyOrders,
}

/// Most orders one [`ToGame::Orders`](crate::ToGame::Orders) may carry.
/// Every order past it is refused with [`RejectReason::TooManyOrders`].
pub const MAX_ORDERS_PER_MESSAGE: usize = 64;

/// Most characters one chat line may carry. A longer one is refused.
pub const MAX_CHAT_CHARS: usize = 200;

/// One order and the unit it is for.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UnitOrder {
    /// The unit that obeys. Given by a participant, it has to be one a seat
    /// of that connection drives; told back, its seat is the unit's owner.
    pub unit: EntityId,
    /// What to do.
    pub order: Order,
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
