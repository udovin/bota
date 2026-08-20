//! What the client knows about abilities, items, effects and heroes.
//!
//! One entry to a thing, found by the id the wire carries. Anything the view
//! already brings -- level, mana cost, cooldown left, charges left -- is read
//! from the view; what stands here is what the wire does not send.

/// How something is aimed when it is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aim {
    /// At nothing: it works on whoever used it.
    Own,
    /// At a spot on the ground.
    Point,
    /// At a unit.
    Unit,
}

/// One ability, as the client shows and aims it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbilityFace {
    /// The id it answers to on the wire.
    pub id: u16,
    /// Short name for the panel.
    pub name: &'static str,
    /// What it does, for the hover popup.
    pub blurb: &'static str,
    /// The drawing of it. Absent while there is none.
    pub icon: Option<&'static [u8]>,
    /// How a cast of it is aimed.
    pub aim: Aim,
    /// How far it may be levelled.
    pub max_level: u8,
    /// Whether it is an ultimate, and so waits on higher hero levels.
    pub ultimate: bool,
    /// Whether it works on its own and is never cast.
    pub passive: bool,
}

/// One item, as the client shows and aims it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemFace {
    /// The id it answers to on the wire.
    pub id: u16,
    /// Short name for the panel and the shop.
    pub name: &'static str,
    /// Its one-line stats, for the shop row.
    pub stats: &'static str,
    /// What it does, for the hover popup.
    pub blurb: &'static str,
    /// The drawing of it. Absent while there is none.
    pub icon: Option<&'static [u8]>,
    /// How a use of it is aimed.
    pub aim: Aim,
    /// What the shop asks for it, in gold.
    pub cost: i32,
    /// Whether the spot it is aimed at means the tree standing there.
    pub at_a_tree: bool,
}

/// One timed effect, as the client shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectFace {
    /// The id it answers to on the wire.
    pub id: u16,
    /// Short name for the chip.
    pub name: &'static str,
    /// What it does, for the hover popup.
    pub blurb: &'static str,
    /// The drawing of it. Absent while there is none.
    pub icon: Option<&'static [u8]>,
}

/// One hero, as the client shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeroFace {
    /// The id it answers to on the wire.
    pub id: u16,
    /// Its name, for the roster and the panel.
    pub name: &'static str,
    /// The drawing of it. Absent while there is none.
    pub icon: Option<&'static [u8]>,
}

/// Every ability, in id order.
pub const ABILITIES: [AbilityFace; 13] = [
    AbilityFace {
        id: 0,
        name: "Crit",
        blurb: "Passive. 20/25/30/35% chance to strike for 175/200/225/250% damage.",
        icon: None,
        aim: Aim::Own,
        max_level: 4,
        ultimate: false,
        passive: true,
    },
    AbilityFace {
        id: 1,
        name: "Frenzy",
        blurb: "No target. +20/28/36/44% attack speed for 6 s. 30/40/50/60 mana.",
        icon: None,
        aim: Aim::Own,
        max_level: 4,
        ultimate: false,
        passive: false,
    },
    AbilityFace {
        id: 2,
        name: "Bounce",
        blurb: "Enemy target, range 550. 70/140/210/280 magic damage, then jumps to the 2/4/6/8 nearest new enemies.",
        icon: None,
        aim: Aim::Unit,
        max_level: 4,
        ultimate: false,
        passive: false,
    },
    AbilityFace {
        id: 3,
        name: "Volley",
        blurb: "Ultimate, no target. An attack at 80/100/120% damage flies at every enemy within 700.",
        icon: None,
        aim: Aim::Own,
        max_level: 3,
        ultimate: true,
        passive: false,
    },
    AbilityFace {
        id: 4,
        name: "Hook",
        blurb: "Point target, range 1100. Catches the first unit in its way, drags it back and deals 90/180/270/360 pure damage.",
        icon: None,
        aim: Aim::Point,
        max_level: 4,
        ultimate: false,
        passive: false,
    },
    AbilityFace {
        id: 5,
        name: "Rot",
        blurb: "Toggle. Burns everything within 250 for 30/60/90/120 a second and slows it, its owner included, but never kills its owner.",
        icon: None,
        aim: Aim::Own,
        max_level: 4,
        ultimate: false,
        passive: false,
    },
    AbilityFace {
        id: 6,
        name: "Heap",
        blurb: "Passive. Magic resistance, and health for every death near you.",
        icon: None,
        aim: Aim::Own,
        max_level: 4,
        ultimate: false,
        passive: true,
    },
    AbilityFace {
        id: 7,
        name: "Dismem",
        blurb: "Ultimate, enemy target, range 150. Holds it for 3 s, eating it and healing you.",
        icon: None,
        aim: Aim::Unit,
        max_level: 3,
        ultimate: true,
        passive: false,
    },
    AbilityFace {
        id: 8,
        name: "Burst",
        blurb: "No target. The courier flies 50% faster for 6 s. 120 s wait.",
        icon: None,
        aim: Aim::Own,
        max_level: 1,
        ultimate: false,
        passive: false,
    },
    AbilityFace {
        id: 9,
        name: "Return",
        blurb: "No target. The courier puts what it holds back in the stash, then goes home.",
        icon: None,
        aim: Aim::Own,
        max_level: 1,
        ultimate: false,
        passive: false,
    },
    AbilityFace {
        id: 10,
        name: "Stash",
        blurb: "No target. The courier takes what waits in your stash and carries it to you.",
        icon: None,
        aim: Aim::Own,
        max_level: 1,
        ultimate: false,
        passive: false,
    },
    AbilityFace {
        id: 11,
        name: "Give",
        blurb: "No target. The courier carries what it holds to you, then goes home.",
        icon: None,
        aim: Aim::Own,
        max_level: 1,
        ultimate: false,
        passive: false,
    },
    AbilityFace {
        id: 12,
        name: "Shield",
        blurb: "No target. Nothing gets through to the courier for 2 s. 200 s wait.",
        icon: None,
        aim: Aim::Own,
        max_level: 1,
        ultimate: false,
        passive: false,
    },
];

/// Every item the shop sells, in id order.
pub const ITEMS: [ItemFace; 9] = [
    ItemFace {
        id: 0,
        name: "Boots",
        stats: "+45 MS",
        blurb: "+45 movement speed.",
        icon: Some(include_bytes!("../assets/items/boots.svg")),
        aim: Aim::Own,
        cost: 500,
        at_a_tree: false,
    },
    ItemFace {
        id: 1,
        name: "Clarity",
        stats: "150MP/25s",
        blurb: "Consumable. Restores 150 mana over 25 s. Any hero's hit breaks it.",
        icon: Some(include_bytes!("../assets/items/clarity.svg")),
        aim: Aim::Unit,
        cost: 50,
        at_a_tree: false,
    },
    ItemFace {
        id: 2,
        name: "Salve",
        stats: "400HP/10s",
        blurb: "Consumable. Restores 400 health over 10 s. Any hero's hit breaks it.",
        icon: Some(include_bytes!("../assets/items/healing_salve.svg")),
        aim: Aim::Unit,
        cost: 110,
        at_a_tree: false,
    },
    ItemFace {
        id: 3,
        name: "Branch",
        stats: "+30HP+15MP",
        blurb: "+30 maximum health, +15 maximum mana, +1 attack damage.",
        icon: Some(include_bytes!("../assets/items/iron_branch.svg")),
        aim: Aim::Point,
        cost: 50,
        at_a_tree: false,
    },
    ItemFace {
        id: 4,
        name: "Obs",
        stats: "Vision",
        blurb: "Consumable. Stands a ward that sees 1600 and that the enemy cannot see.",
        icon: Some(include_bytes!("../assets/items/observer_ward.svg")),
        aim: Aim::Point,
        cost: 100,
        at_a_tree: false,
    },
    ItemFace {
        id: 5,
        name: "Quell",
        stats: "+18 creep",
        blurb: "+18 attack damage against creeps. Fells the tree you point at.",
        icon: Some(include_bytes!("../assets/items/quelling_blade.svg")),
        aim: Aim::Point,
        cost: 225,
        at_a_tree: true,
    },
    ItemFace {
        id: 6,
        name: "Sentry",
        stats: "True sight",
        blurb: "Consumable. Stands a ward that gives true sight, revealing enemy wards.",
        icon: Some(include_bytes!("../assets/items/sentry_ward.svg")),
        aim: Aim::Point,
        cost: 50,
        at_a_tree: false,
    },
    ItemFace {
        id: 7,
        name: "Tango",
        stats: "115HP x3",
        blurb: "Three charges. Eats a tree to restore 115 health over 16 s.",
        icon: Some(include_bytes!("../assets/items/tango.svg")),
        aim: Aim::Point,
        cost: 90,
        at_a_tree: true,
    },
    ItemFace {
        id: 8,
        name: "TP",
        stats: "Teleport",
        blurb: "Consumable. Channels, then carries you to an allied building.",
        icon: Some(include_bytes!("../assets/items/town_portal_scroll.svg")),
        aim: Aim::Point,
        cost: 100,
        at_a_tree: false,
    },
];

/// Every timed effect, in id order.
pub const EFFECTS: [EffectFace; 7] = [
    EffectFace {
        id: 0,
        name: "Frenzy",
        blurb: "Attack speed increased.",
        icon: None,
    },
    EffectFace {
        id: 1,
        name: "Mending",
        blurb: "Regenerating health.",
        icon: None,
    },
    EffectFace {
        id: 2,
        name: "Clarity",
        blurb: "Regenerating mana.",
        icon: None,
    },
    EffectFace {
        id: 3,
        name: "Fountain",
        blurb: "Regenerating health and mana for standing in the fountain.",
        icon: None,
    },
    EffectFace {
        id: 4,
        name: "Held",
        blurb: "Cannot move, attack or cast.",
        icon: None,
    },
    EffectFace {
        id: 5,
        name: "Slowed",
        blurb: "Movement speed reduced.",
        icon: None,
    },
    EffectFace {
        id: 6,
        name: "Burning",
        blurb: "Losing health over time.",
        icon: None,
    },
];

/// Every hero that can be picked, in id order.
pub const HEROES: [HeroFace; 2] = [
    HeroFace {
        id: 0,
        name: "Sylla",
        icon: None,
    },
    HeroFace {
        id: 1,
        name: "Pudge",
        icon: None,
    },
];

/// Item id of the Town Portal Scroll.
pub const TOWN_PORTAL_SCROLL: u16 = 8;

/// The ability of that id, or nothing for one the catalog does not hold.
pub fn ability(id: u16) -> Option<&'static AbilityFace> {
    ABILITIES.get(usize::from(id))
}

/// The item of that id, or nothing for one the catalog does not hold.
pub fn item(id: u16) -> Option<&'static ItemFace> {
    ITEMS.get(usize::from(id))
}

/// The effect of that id, or nothing for one the catalog does not hold.
pub fn effect(id: u16) -> Option<&'static EffectFace> {
    EFFECTS.get(usize::from(id))
}

/// The hero of that id, or nothing for one the catalog does not hold.
pub fn hero(id: u16) -> Option<&'static HeroFace> {
    HEROES.get(usize::from(id))
}

/// How an ability is aimed, falling back to no target for an unknown one.
pub fn ability_aim(id: u16) -> Aim {
    ability(id).map_or(Aim::Own, |face| face.aim)
}

/// How an item is aimed, falling back to no target for an unknown one.
pub fn item_aim(id: u16) -> Aim {
    item(id).map_or(Aim::Own, |face| face.aim)
}

/// How far an ability may be levelled, whatever slot it sits in.
pub fn ability_cap(id: u16) -> u8 {
    ability(id).map_or(0, |face| face.max_level)
}
