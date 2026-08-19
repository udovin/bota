//! What an entity carries.

use bota_proto::ItemId;

/// One item in a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemStack {
    /// What it is.
    pub id: ItemId,
    /// Uses left, for a consumable. Zero for one that only carries bonuses.
    pub charges: u8,
    /// Ticks until it may be used again.
    pub cooldown: u32,
    /// The tick it was bought, for the window in which it refunds in full.
    pub bought_tick: u32,
    /// Whether it has been used or moved since it was bought.
    pub touched: bool,
}

/// The slots an entity carries items in.
///
/// A slot holding nothing is `None`, so slots keep their numbers as items come
/// and go.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inventory {
    /// Every slot, in the order they are shown.
    pub slots: Vec<Option<ItemStack>>,
}

impl Inventory {
    /// An inventory of that many empty slots.
    pub fn empty(slots: usize) -> Inventory {
        Inventory {
            slots: vec![None; slots],
        }
    }

    /// Every item held, in slot order.
    pub fn held(&self) -> impl Iterator<Item = &ItemStack> {
        self.slots.iter().flatten()
    }
}
