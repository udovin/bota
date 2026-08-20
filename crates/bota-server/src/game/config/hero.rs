//! Every hero that may be picked, and what each one carries.

use bota_proto::{AbilityId, HeroId};

use crate::game::{HERO, PUDGE, UnitDef, ability};

/// One hero: what it is made of and what it can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeroDef {
    /// What it is called.
    pub name: &'static str,
    /// The plain form of its body.
    pub unit: &'static UnitDef,
    /// The four slots it carries, in the order they are shown.
    pub abilities: [AbilityId; 4],
}

/// Every hero, indexed by [`HeroId`].
pub const HEROES: [HeroDef; 2] = [
    HeroDef {
        name: "Sylla",
        unit: &HERO,
        abilities: [
            ability::CRIT,
            ability::FRENZY,
            ability::BOUNCE,
            ability::VOLLEY,
        ],
    },
    HeroDef {
        name: "Pudge",
        unit: &PUDGE,
        abilities: [
            ability::MEAT_HOOK,
            ability::ROT,
            ability::FLESH_HEAP,
            ability::DISMEMBER,
        ],
    },
];

/// What one hero is, or nothing if no such hero exists.
pub fn hero_def(id: HeroId) -> Option<&'static HeroDef> {
    HEROES.get(usize::from(id.0))
}
