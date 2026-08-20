//! One thing the bot wants, before it becomes an order.
//!
//! The server keeps one order per seat per tick, so wanting two things at once
//! means wanting them in turn. A want is what the policy hands back; turning it
//! into an [`Order`] and deciding whether it is worth the tick is done in one
//! place, on the way out.

use bota_proto::{AbilitySlot, EntityId, ItemId, ItemSlot, Order, OrderTarget, Vec2};

use crate::span;

/// One order, and which unit it is for.
///
/// Most orders are for the seat's own hero and name nobody; an errand names
/// the courier it is for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ask {
    /// Whom it is for. Absent means the hero.
    pub unit: Option<EntityId>,
    /// What to do.
    pub order: Order,
}

/// What the bot would do this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Want {
    /// Put the spare skill point into a slot.
    Level(AbilitySlot),
    /// Buy something from the shop it is standing in.
    Buy(ItemId),
    /// Move something between slots, to take it out of the stash.
    Fetch {
        /// The slot it comes from.
        from: ItemSlot,
        /// The slot it goes to.
        to: ItemSlot,
    },
    /// Use something it carries.
    Use {
        /// Which slot holds it.
        slot: ItemSlot,
        /// What it is aimed at.
        at: OrderTarget,
    },
    /// Cast one of its abilities.
    Cast {
        /// Which of the four slots.
        slot: AbilitySlot,
        /// What it is aimed at.
        at: OrderTarget,
    },
    /// Swing at one body until it falls or leaves.
    Hit(EntityId),
    /// Walk somewhere, stopping for whatever it meets.
    Push(Vec2),
    /// Walk somewhere, paying attention to nothing on the way.
    Walk(Vec2),
    /// Stand still and take on whatever comes near.
    Hold,
    /// Stand still and pay attention to nobody.
    Stop,
    /// Send the courier on one of the errands it carries.
    Errand {
        /// Which courier.
        courier: EntityId,
        /// Which of its errands.
        slot: AbilitySlot,
    },
}

impl Want {
    /// The whole of what goes on the wire: the order, and whom it is for.
    pub fn ask(self) -> Ask {
        Ask {
            unit: self.for_unit(),
            order: self.order(),
        }
    }

    /// The unit the order is for. Absent means the seat's own hero, which is
    /// what all but an errand are.
    pub fn for_unit(self) -> Option<EntityId> {
        match self {
            Want::Errand { courier, .. } => Some(courier),
            _ => None,
        }
    }

    /// The order that asks for it.
    pub fn order(self) -> Order {
        match self {
            Want::Level(slot) => Order::LevelUpAbility { slot },
            Want::Buy(item) => Order::BuyItem { item },
            Want::Fetch { from, to } => Order::MoveItem { from, to },
            Want::Use { slot, at } => Order::UseItem { slot, target: at },
            Want::Cast { slot, at } => Order::CastAbility { slot, target: at },
            Want::Hit(target) => Order::AttackUnit { target },
            Want::Push(pos) => Order::AttackMove { pos },
            Want::Walk(pos) => Order::Move { pos },
            Want::Hold => Order::HoldPosition,
            Want::Stop => Order::Stop,
            Want::Errand { slot, .. } => Order::CastAbility {
                slot,
                target: crate::AIMED_AT,
            },
        }
    }

    /// Whether this is the want that is already standing.
    ///
    /// Two walks to spots less than `drift` apart are one want: the wave the
    /// bot follows never stops moving, and a fresh order every tick throws away
    /// the route the server laid for the last one.
    pub fn same_as(self, other: Want, drift: f32) -> bool {
        match (self, other) {
            (Want::Push(one), Want::Push(other)) | (Want::Walk(one), Want::Walk(other)) => {
                span(one, other) <= drift
            }
            _ => self == other,
        }
    }

    /// Whether the want asks the body to go somewhere.
    pub fn is_a_walk(self) -> bool {
        matches!(self, Want::Push(_) | Want::Walk(_))
    }
}
