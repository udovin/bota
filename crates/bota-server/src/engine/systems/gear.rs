//! Items carried and abilities held: what they add, and what they cost.

use bota_proto::{AbilityId, EventKind, ItemId, OrderTarget, SlotId};

use crate::engine::{
    AbilityBook, AbilityState, BAG_SLOTS, Carried, Entity, Inventory, ItemStack, ItemUse, Pool,
    Status, StatusKind, World, in_backpack, in_inventory, in_stash, item_def, wire_id,
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

/// What a whole inventory adds.
///
/// Only what sits in the inventory proper counts; what is in the backpack is
/// carried inert.
pub fn carried_bonus(inventory: &Inventory) -> Carried {
    let mut total = Carried::default();
    for stack in inventory
        .slots
        .iter()
        .take(rules::INVENTORY_SLOTS)
        .flatten()
    {
        if stack.mute > 0 {
            continue;
        }
        let Some(def) = item_def(stack.id) else {
            continue;
        };
        total.move_speed += def.carried.move_speed;
        total.damage += def.carried.damage;
        total.armor += def.carried.armor;
        total.hp += def.carried.hp;
        total.mana += def.carried.mana;
        total.damage_to_creeps += def.carried.damage_to_creeps;
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
                    stack.mute = stack.mute.saturating_sub(1);
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

    /// Buys an item for a seat that can afford it.
    ///
    /// At its own shop it goes to the first free slot of the hero's bag, and
    /// to the stash when that bag is full. Anywhere else it goes to the stash
    /// and waits there.
    pub fn buy(&mut self, slot: SlotId, item: ItemId, events: &mut Vec<Event>) -> bool {
        let Some(index) = self.seats.iter().position(|s| s.slot == slot) else {
            return false;
        };
        let Some(unit) = self.seats[index].unit else {
            return false;
        };
        let Some(def) = item_def(item) else {
            return false;
        };
        if self.seats[index].gold < def.cost {
            return false;
        }
        let bought = ItemStack {
            id: item,
            charges: def.charges,
            cooldown: 0,
            mute: 0,
            bought_tick: self.tick,
            touched: false,
        };
        let in_hand = self.at_shop(unit)
            && self
                .inventory
                .get_mut(unit)
                .and_then(|bag| bag.slots.iter_mut().find(|slot| slot.is_none()))
                .map(|free| *free = Some(bought))
                .is_some();
        if !in_hand && !self.put_in_stash(index, bought) {
            return false;
        }
        self.seats[index].gold -= def.cost;
        events.push(Event {
            kind: EventKind::ItemBought { slot, item },
            visible_to: EventVisibility::OneTeam(self.seats[index].team),
        });
        true
    }

    /// Lays a stack in the first free slot of a seat's stash.
    fn put_in_stash(&mut self, seat: usize, stack: ItemStack) -> bool {
        let Some(free) = self.seats[seat]
            .stash
            .slots
            .iter_mut()
            .find(|slot| slot.is_none())
        else {
            return false;
        };
        *free = Some(stack);
        true
    }

    /// Uses what sits in one of an entity's item slots.
    ///
    /// A use that does nothing spends nothing: the charge, the cooldown and
    /// the slot are only touched once whatever the item does has been done.
    pub fn use_item(&mut self, entity: Entity, slot: usize, target: OrderTarget) -> bool {
        let Some(bag) = self.inventory.get(entity) else {
            return false;
        };
        let Some(Some(stack)) = bag.slots.get(slot).cloned() else {
            return false;
        };
        if slot >= rules::INVENTORY_SLOTS || stack.cooldown > 0 || stack.mute > 0 {
            return false;
        }
        let Some(def) = item_def(stack.id) else {
            return false;
        };
        if def.charges > 0 && stack.charges == 0 {
            return false;
        }
        let Some(active) = def.active else {
            return false;
        };
        let done = match active {
            ItemUse::Mend {
                pool,
                total,
                ticks,
                range,
                eats_a_tree,
            } => self.mend_with(entity, target, pool, total, ticks, range, eats_a_tree),
            ItemUse::Teleport { channel, range } => {
                self.begin_teleport(entity, target, channel, range, slot)
            }
            ItemUse::Ward { def, ticks, range } => {
                self.stand_ward(entity, target, def, ticks, range)
            }
            ItemUse::Fell { .. } => false,
        };
        if !done {
            return false;
        }
        let cooldown = def.cooldown;
        // A scroll is spent when it carries, which is the teleport's own
        // business; everything else is spent the moment it is used.
        let spends = def.charges > 0 && !matches!(active, ItemUse::Teleport { .. });
        if let Some(bag) = self.inventory.get_mut(entity)
            && let Some(held) = bag.slots.get_mut(slot)
            && let Some(stack) = held
        {
            stack.cooldown = cooldown;
            stack.touched = true;
            if spends {
                stack.charges = stack.charges.saturating_sub(1);
                if stack.charges == 0 {
                    *held = None;
                }
            }
        }
        true
    }

    /// Puts a mending effect on whoever an item was used on.
    ///
    /// It reaches one of its user's own side, standing within `range`. Aimed
    /// at nothing at all, it lands on the one who used it.
    #[allow(clippy::too_many_arguments)]
    fn mend_with(
        &mut self,
        user: Entity,
        target: OrderTarget,
        pool: Pool,
        total: i32,
        ticks: u32,
        range: i32,
        eats_a_tree: bool,
    ) -> bool {
        let on = match target {
            OrderTarget::Unit { target } => match self.of_wire(target) {
                Some(on) => on,
                None => return false,
            },
            OrderTarget::None | OrderTarget::Point { .. } => user,
        };
        if !self.alive(on) || self.team.get(on) != self.team.get(user) {
            return false;
        }
        let (Some(from), Some(at)) = (
            self.transform.get(user).map(|t| t.pos),
            self.transform.get(on).map(|t| t.pos),
        ) else {
            return false;
        };
        if !from.within(at, rules::units(range)) {
            return false;
        }
        if eats_a_tree {
            return false;
        }
        let per_tick = total * 100 / ticks.max(1) as i32;
        let put = Status {
            kind: match pool {
                Pool::Health => StatusKind::Mending { per_tick },
                Pool::Mana => StatusKind::Clarity { per_tick },
            },
            ticks_left: ticks,
        };
        match self.statuses.get_mut(on) {
            Some(on_it) => on_it.put(put),
            None => {
                let mut on_it = crate::engine::Statuses::default();
                on_it.put(put);
                self.statuses.insert(on, on_it);
            }
        }
        true
    }

    /// Burns one charge of what sits in a slot, clearing the slot when the
    /// last one goes.
    pub fn spend_charge(&mut self, entity: Entity, slot: usize) {
        if let Some(bag) = self.inventory.get_mut(entity)
            && let Some(held) = bag.slots.get_mut(slot)
            && let Some(stack) = held
        {
            stack.touched = true;
            stack.charges = stack.charges.saturating_sub(1);
            if stack.charges == 0 {
                *held = None;
            }
        }
    }

    /// Moves what sits in one slot to another, swapping whatever is in the
    /// way.
    ///
    /// Slots run inventory, then backpack, then stash. The stash takes part
    /// only while the hero stands in its own shop. A stack coming out of the
    /// backpack into the inventory is muted for a while.
    pub fn move_item(&mut self, slot: SlotId, from: usize, to: usize) -> bool {
        let Some(seat) = self.seats.iter().position(|s| s.slot == slot) else {
            return false;
        };
        let Some(unit) = self.seats[seat].unit else {
            return false;
        };
        let total = BAG_SLOTS + self.seats[seat].stash.slots.len();
        if from == to || from >= total || to >= total {
            return false;
        }
        if (in_stash(from) || in_stash(to)) && !self.at_shop(unit) {
            return false;
        }
        let Some(moved) = self.take_slot(unit, seat, from) else {
            return false;
        };
        let displaced = self.take_slot(unit, seat, to);
        self.put_slot(unit, seat, to, Some(moved));
        self.put_slot(unit, seat, from, displaced);
        for (origin, landed) in [(from, to), (to, from)] {
            let mute = in_backpack(origin) && in_inventory(landed);
            if let Some(stack) = self.slot_mut(unit, seat, landed) {
                stack.touched = true;
                if mute {
                    stack.mute = stack.mute.max(rules::BACKPACK_MUTE_TICKS);
                }
            }
        }
        true
    }

    /// Sells what sits in one slot, at its own shop.
    ///
    /// An untouched stack sold soon after it was bought pays back what it
    /// cost; anything else pays back a part of it.
    pub fn sell_item(&mut self, slot: SlotId, at: usize) -> bool {
        let Some(seat) = self.seats.iter().position(|s| s.slot == slot) else {
            return false;
        };
        let Some(unit) = self.seats[seat].unit else {
            return false;
        };
        if !self.at_shop(unit) {
            return false;
        }
        let Some(stack) = self.take_slot(unit, seat, at) else {
            return false;
        };
        let Some(def) = item_def(stack.id) else {
            self.put_slot(unit, seat, at, Some(stack));
            return false;
        };
        let fresh = !stack.touched
            && self.tick.saturating_sub(stack.bought_tick) <= rules::SELL_REFUND_TICKS;
        let back = if fresh {
            def.cost
        } else {
            def.cost * rules::SELL_PCT / 100
        };
        self.seats[seat].gold += back;
        true
    }

    /// Takes whatever sits in one slot of a seat's slots out of it.
    fn take_slot(&mut self, unit: Entity, seat: usize, slot: usize) -> Option<ItemStack> {
        if in_stash(slot) {
            self.seats[seat]
                .stash
                .slots
                .get_mut(slot - BAG_SLOTS)?
                .take()
        } else {
            self.inventory.get_mut(unit)?.slots.get_mut(slot)?.take()
        }
    }

    /// Lays a stack in one slot, over whatever was there.
    fn put_slot(&mut self, unit: Entity, seat: usize, slot: usize, stack: Option<ItemStack>) {
        if let Some(held) = self.slot_of(unit, seat, slot) {
            *held = stack;
        }
    }

    /// The stack in one slot, to be changed in place.
    fn slot_mut(&mut self, unit: Entity, seat: usize, slot: usize) -> Option<&mut ItemStack> {
        self.slot_of(unit, seat, slot)?.as_mut()
    }

    /// One slot of a seat's slots, wherever it lives.
    fn slot_of(
        &mut self,
        unit: Entity,
        seat: usize,
        slot: usize,
    ) -> Option<&mut Option<ItemStack>> {
        if in_stash(slot) {
            self.seats[seat].stash.slots.get_mut(slot - BAG_SLOTS)
        } else {
            self.inventory.get_mut(unit)?.slots.get_mut(slot)
        }
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
