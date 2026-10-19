//! What items are called: the names the shop shows and a chat command reads.

use crate::ItemId;

/// Every item's name, indexed by [`ItemId`].
pub const ITEM_NAMES: [&str; 52] = [
    "Boots",
    "Clarity",
    "Salve",
    "Branch",
    "Obs",
    "Quell",
    "Sentry",
    "Tango",
    "TP",
    "Circlet",
    "Gauntlet",
    "Slippers",
    "Mantle",
    "Belt",
    "Band",
    "Robe",
    "Ogre Axe",
    "Alacrity",
    "Wizardry",
    "Gloves",
    "Blades",
    "Broad",
    "Qstaff",
    "Ring Pro",
    "Chain",
    "Regen",
    "Sage",
    "Vitality",
    "Energy",
    "Treads",
    "Phase",
    "Blink",
    "Bracer",
    "Wraith",
    "Null",
    "Stick",
    "Wand",
    "Rcp Phase",
    "Rcp Bracer",
    "Rcp Wraith",
    "Rcp Null",
    "Rcp Wand",
    "Mango",
    "Eaglesong",
    "Claymore",
    "Talisman",
    "Butterfly",
    "Javelin",
    "Demon Edge",
    "Blitz",
    "Rcp MKB",
    "MKB",
];

/// What an item is called. Absent for an id no item has.
pub fn item_name(id: ItemId) -> Option<&'static str> {
    ITEM_NAMES.get(usize::from(id.0)).copied()
}

/// The item a name stands for, case aside. Absent when none is called that.
pub fn item_called(name: &str) -> Option<ItemId> {
    ITEM_NAMES
        .iter()
        .position(|known| known.eq_ignore_ascii_case(name))
        .map(|at| ItemId(at as u16))
}
