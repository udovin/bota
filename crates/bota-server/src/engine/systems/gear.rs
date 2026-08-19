//! Items carried and abilities held: what they add, and what they cost.

use bota_proto::{AbilityId, EventKind, ItemId, SlotId};

use crate::engine::{
    AbilityBook, AbilityState, Entity, Inventory, ItemStack, Status, StatusKind, World, wire_id,
};
use crate::sim::{Event, EventVisibility, rules};

/// What every hero can cast until there is more than one hero.
pub fn hero_kit() -> AbilityBook {
    AbilityBook {
        slots: (0..4)
            .map(|slot| AbilityState {
                id: AbilityId(slot),
                level: 0,
                cooldown: 0,
            })
            .collect(),
    }
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
}

/// What a whole inventory adds.
///
/// An item with charges carries nothing; it is spent instead. One on cooldown
/// carries nothing either, which is what muting a backpacked item means.
pub fn carried_bonus(inventory: &Inventory) -> Carried {
    let mut total = Carried::default();
    for stack in inventory
        .slots
        .iter()
        .take(rules::INVENTORY_SLOTS)
        .flatten()
    {
        if stack.cooldown > 0 {
            continue;
        }
        let def = &rules::ITEMS[usize::from(stack.id.0)];
        if def.charges > 0 {
            continue;
        }
        total.move_speed += def.move_speed;
        total.damage += def.damage;
        total.armor += def.armor;
        total.hp += def.hp;
        total.mana += def.mana;
    }
    total
}

impl World {
    /// Runs down every cooldown an entity is waiting on.
    pub fn tick_gear(&mut self) {
        for entity in self.entities.iter().collect::<Vec<_>>() {
            if let Some(book) = self.abilities.get_mut(entity) {
                for slot in book.slots.iter_mut() {
                    slot.cooldown = slot.cooldown.saturating_sub(1);
                }
            }
            if let Some(bag) = self.inventory.get_mut(entity) {
                for stack in bag.slots.iter_mut().flatten() {
                    stack.cooldown = stack.cooldown.saturating_sub(1);
                }
            }
            if let Some(on_it) = self.statuses.get_mut(entity) {
                for status in on_it.0.iter_mut() {
                    status.ticks_left = status.ticks_left.saturating_sub(1);
                }
                on_it.0.retain(|status| status.ticks_left > 0);
            }
        }
    }

    /// Buys an item for a seat that can afford it and whose hero stands in
    /// its own shop.
    ///
    /// The item goes to the first free slot of the hero's own bag.
    pub fn buy(&mut self, slot: SlotId, item: ItemId, events: &mut Vec<Event>) -> bool {
        let Some(index) = self.seats.iter().position(|s| s.slot == slot) else {
            return false;
        };
        let Some(unit) = self.seats[index].unit else {
            return false;
        };
        if !self.at_shop(unit) {
            return false;
        }
        let cost = rules::ITEMS[usize::from(item.0)].cost;
        if self.seats[index].gold < cost {
            return false;
        }
        let charges = rules::ITEMS[usize::from(item.0)].charges;
        let bought = ItemStack {
            id: item,
            charges,
            cooldown: 0,
            bought_tick: self.tick,
            touched: false,
        };
        let Some(bag) = self.inventory.get_mut(unit) else {
            return false;
        };
        let Some(free) = bag.slots.iter_mut().find(|s| s.is_none()) else {
            return false;
        };
        *free = Some(bought);
        self.seats[index].gold -= cost;
        events.push(Event {
            kind: EventKind::ItemBought { slot, item },
            visible_to: EventVisibility::OneTeam(self.seats[index].team),
        });
        true
    }

    /// Uses what sits in one of an entity's item slots.
    ///
    /// A consumable spends a charge and puts what it does on whoever used it;
    /// an empty stack is cleared away.
    pub fn use_item(&mut self, entity: Entity, slot: usize) -> bool {
        let Some(bag) = self.inventory.get(entity) else {
            return false;
        };
        let Some(Some(stack)) = bag.slots.get(slot).cloned() else {
            return false;
        };
        if stack.cooldown > 0 || stack.charges == 0 {
            return false;
        }
        let put = match stack.id.0 {
            rules::ITEM_SALVE => Some(Status {
                kind: StatusKind::Mending,
                ticks_left: rules::REGEN_BUFF_TICKS,
                magnitude: rules::SALVE_HP_PER_PULSE * 100 / rules::SALVE_PULSE_TICKS as i32,
            }),
            rules::ITEM_CLARITY => Some(Status {
                kind: StatusKind::Clarity,
                ticks_left: rules::REGEN_BUFF_TICKS,
                magnitude: rules::CLARITY_MANA_PER_PULSE * 100 / rules::CLARITY_PULSE_TICKS as i32,
            }),
            _ => None,
        };
        let Some(status) = put else {
            return false;
        };
        let mut on_it = self.statuses.remove(entity).unwrap_or_default();
        on_it.0.retain(|held| held.kind != status.kind);
        on_it.0.push(status);
        self.statuses.insert(entity, on_it);
        if let Some(bag) = self.inventory.get_mut(entity)
            && let Some(held) = bag.slots.get_mut(slot)
            && let Some(stack) = held
        {
            stack.charges = stack.charges.saturating_sub(1);
            stack.touched = true;
            if stack.charges == 0 {
                *held = None;
            }
        }
        true
    }

    /// Puts an ability point into one slot of an entity's book.
    pub fn learn(&mut self, entity: Entity, slot: usize, events: &mut Vec<Event>) -> bool {
        let Some(book) = self.abilities.get_mut(entity) else {
            return false;
        };
        let Some(ability) = book.slots.get_mut(slot) else {
            return false;
        };
        if ability.level >= rules::ABILITY_MAX_LEVEL {
            return false;
        }
        ability.level += 1;
        let id = ability.id;
        events.push(Event {
            kind: EventKind::AbilityCast {
                caster: wire_id(entity),
                ability: id,
            },
            visible_to: EventVisibility::Everyone,
        });
        true
    }
}
