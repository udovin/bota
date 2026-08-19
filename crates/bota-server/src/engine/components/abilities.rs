//! What an entity can cast.

use bota_proto::AbilityId;

/// One ability in a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbilityState {
    /// Which ability sits here.
    pub id: AbilityId,
    /// Level it stands at. Zero until it is learned.
    pub level: u8,
    /// Ticks until it may be cast again.
    pub cooldown: u32,
}

/// The abilities an entity holds, in slot order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AbilityBook {
    /// Every slot, in the order they are shown.
    pub slots: Vec<AbilityState>,
}

impl AbilityBook {
    /// What sits in a slot, if anything does.
    pub fn slot(&self, index: usize) -> Option<&AbilityState> {
        self.slots.get(index)
    }

    /// Every ability that has been learned.
    pub fn learned(&self) -> impl Iterator<Item = &AbilityState> {
        self.slots.iter().filter(|a| a.level > 0)
    }
}
