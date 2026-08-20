//! The catalog: what an item costs, what it carries, and what using it does.
//!
//! Every entry answers to an [`ItemId`], which is its place in [`ITEMS`].

use bota_proto::{ItemId, ItemView};

use crate::game::Inventory;

use crate::game::UnitDef;
use crate::game::rules;

/// Which pool an item mends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pool {
    /// Health.
    Health,
    /// Mana.
    Mana,
}

/// What one item adds to whoever carries it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Carried {
    /// Movement speed added.
    pub move_speed: i32,
    /// Attack damage added.
    pub damage: i32,
    /// Armor added.
    pub armor: i32,
    /// Maximum health added.
    pub hp: i32,
    /// Maximum mana added.
    pub mana: i32,
    /// Attack damage added against anything that is not a hero.
    pub damage_to_creeps: i32,
}

/// Nothing carried at all, so an entry names only what it adds.
const NOTHING: Carried = Carried {
    move_speed: 0,
    damage: 0,
    armor: 0,
    hp: 0,
    mana: 0,
    damage_to_creeps: 0,
};

/// What using an item does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemUse {
    /// Mends a unit over time.
    Mend {
        /// Which pool it mends.
        pool: Pool,
        /// How much it mends over the whole of it.
        total: i32,
        /// How long it runs.
        ticks: u32,
        /// How far it reaches, in world units.
        range: i32,
        /// Whether it takes a tree down to work, and needs one in reach.
        eats_a_tree: bool,
        /// Whether a blow from a hero, a tower or Roshan puts it out.
        breaks: bool,
    },
    /// Stands a ward at a point.
    Ward {
        /// What kind of ward it stands.
        def: &'static UnitDef,
        /// How long the ward stands.
        ticks: u32,
        /// How far it reaches, in world units.
        range: i32,
    },
    /// Takes a tree down.
    Fell {
        /// How far it reaches, in world units.
        range: i32,
    },
    /// Puts a tree up, to stand for a while.
    Plant {
        /// Ticks it stands before it goes on its own.
        ticks: u32,
        /// How far it reaches, in world units.
        range: i32,
    },
    /// Carries whoever used it to an allied building.
    Teleport {
        /// Ticks of channelling before it carries.
        channel: u32,
        /// How far from an allied building it may land, in world units.
        range: i32,
    },
}

/// One entry of the catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemDef {
    /// Price in gold.
    pub cost: i32,
    /// Uses it carries. Zero for one that is never used up.
    pub charges: u8,
    /// Ticks before it may be used again.
    pub cooldown: u32,
    /// Whether that wait is owed by whoever used it rather than by the stack:
    /// buying another does not buy a fresh wait.
    pub shared_wait: bool,
    /// What it adds to whoever carries it.
    pub carried: Carried,
    /// What using it does. Absent for one that cannot be used.
    pub active: Option<ItemUse>,
}

/// Boots of Speed.
pub const ITEM_BOOTS: u16 = 0;
/// Clarity.
pub const ITEM_CLARITY: u16 = 1;
/// Healing Salve.
pub const ITEM_HEALING_SALVE: u16 = 2;
/// Iron Branch.
pub const ITEM_IRON_BRANCH: u16 = 3;
/// Observer Ward.
pub const ITEM_OBSERVER_WARD: u16 = 4;
/// Quelling Blade.
pub const ITEM_QUELLING_BLADE: u16 = 5;
/// Sentry Ward.
pub const ITEM_SENTRY_WARD: u16 = 6;
/// Tango.
pub const ITEM_TANGO: u16 = 7;
/// Town Portal Scroll.
pub const ITEM_TOWN_PORTAL_SCROLL: u16 = 8;

/// The catalog, indexed by [`ItemId`].
pub const ITEMS: [ItemDef; 9] = [
    // Boots of Speed.
    ItemDef {
        cost: 500,
        charges: 0,
        cooldown: 0,
        shared_wait: false,
        carried: Carried {
            move_speed: 45,
            ..NOTHING
        },
        active: None,
    },
    // Clarity.
    ItemDef {
        cost: 50,
        charges: 1,
        cooldown: 0,
        shared_wait: false,
        carried: NOTHING,
        active: Some(ItemUse::Mend {
            pool: Pool::Mana,
            total: 150,
            ticks: 750,
            range: 250,
            eats_a_tree: false,
            breaks: true,
        }),
    },
    // Healing Salve.
    ItemDef {
        cost: 110,
        charges: 1,
        cooldown: 0,
        shared_wait: false,
        carried: NOTHING,
        active: Some(ItemUse::Mend {
            pool: Pool::Health,
            total: 400,
            ticks: 300,
            range: 250,
            eats_a_tree: false,
            breaks: true,
        }),
    },
    // Iron Branch.
    ItemDef {
        cost: 50,
        charges: 1,
        cooldown: 0,
        shared_wait: false,
        carried: Carried {
            damage: 1,
            hp: 30,
            mana: 15,
            ..NOTHING
        },
        active: Some(ItemUse::Plant {
            ticks: rules::PLANTED_TREE_TICKS,
            range: 350,
        }),
    },
    // Observer Ward.
    ItemDef {
        cost: 100,
        charges: 1,
        cooldown: 0,
        shared_wait: false,
        carried: NOTHING,
        active: Some(ItemUse::Ward {
            def: &crate::game::OBSERVER_WARD,
            ticks: 10800,
            range: 500,
        }),
    },
    // Quelling Blade.
    ItemDef {
        cost: 225,
        charges: 0,
        cooldown: 120,
        shared_wait: false,
        carried: Carried {
            damage_to_creeps: 18,
            ..NOTHING
        },
        active: Some(ItemUse::Fell { range: 350 }),
    },
    // Sentry Ward.
    ItemDef {
        cost: 50,
        charges: 1,
        cooldown: 0,
        shared_wait: false,
        carried: NOTHING,
        active: Some(ItemUse::Ward {
            def: &crate::game::SENTRY_WARD,
            ticks: 12600,
            range: 500,
        }),
    },
    // Tango.
    ItemDef {
        cost: 90,
        charges: 3,
        cooldown: 0,
        shared_wait: false,
        carried: NOTHING,
        active: Some(ItemUse::Mend {
            pool: Pool::Health,
            total: 115,
            ticks: 480,
            range: 165,
            eats_a_tree: true,
            breaks: false,
        }),
    },
    // Town Portal Scroll.
    ItemDef {
        cost: 100,
        charges: 1,
        cooldown: rules::SCROLL_WAIT_TICKS,
        shared_wait: true,
        carried: NOTHING,
        active: Some(ItemUse::Teleport {
            channel: 90,
            range: 600,
        }),
    },
];

/// What one item is, or nothing if no such item exists.
pub fn item_def(id: ItemId) -> Option<&'static ItemDef> {
    ITEMS.get(usize::from(id.0))
}

/// What a bag looks like on the wire, an empty slot keeping its place.
pub fn item_views(bag: &Inventory) -> Vec<Option<ItemView>> {
    bag.slots
        .iter()
        .map(|slot| {
            slot.map(|stack| ItemView {
                id: stack.id,
                charges: stack.charges,
                cooldown_left: stack.cooldown,
            })
        })
        .collect()
}
