//! The item engine: slots, the shop, the stash and carried bonuses.

use bota_proto::{EntityId, ItemId, ItemSlot, ItemView, RejectReason, SlotId, UnitKind};

use crate::sim::{World, rules};

/// One owned item in a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemStack {
    /// What it is.
    pub id: ItemId,
    /// Uses left, for consumables. Zero for carried bonuses.
    pub charges: u8,
    /// Ticks until usable again; covers the backpack mute.
    pub cooldown: u32,
    /// The tick it was bought, for the full-refund window.
    pub bought_tick: u32,
    /// Whether it has been used or moved since purchase; a touched item
    /// never refunds in full.
    pub touched: bool,
}

/// Total slots a seat owns: inventory, backpack and stash.
pub const TOTAL_SLOTS: usize = rules::INVENTORY_SLOTS + rules::BACKPACK_SLOTS + rules::STASH_SLOTS;

/// Whether a slot index is in the inventory.
pub fn in_inventory(slot: usize) -> bool {
    slot < rules::INVENTORY_SLOTS
}

/// Whether a slot index is in the backpack.
pub fn in_backpack(slot: usize) -> bool {
    (rules::INVENTORY_SLOTS..rules::INVENTORY_SLOTS + rules::BACKPACK_SLOTS).contains(&slot)
}

/// Whether a slot index is in the stash.
pub fn in_stash(slot: usize) -> bool {
    (rules::INVENTORY_SLOTS + rules::BACKPACK_SLOTS..TOTAL_SLOTS).contains(&slot)
}

/// The flat bonuses of everything working in the inventory.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemBonus {
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

/// Sums the bonuses of inventory items that are not muted.
pub fn carried_bonus(items: &[Option<ItemStack>]) -> ItemBonus {
    let mut b = ItemBonus::default();
    for stack in items.iter().take(rules::INVENTORY_SLOTS).flatten() {
        if stack.cooldown > 0 {
            continue;
        }
        let def = &rules::ITEMS[usize::from(stack.id.0)];
        if def.charges > 0 {
            continue;
        }
        b.move_speed += def.move_speed;
        b.damage += def.damage;
        b.armor += def.armor;
        b.hp += def.hp;
        b.mana += def.mana;
    }
    b
}

impl World {
    /// Whether a seat's hero stands in its home shop area.
    pub fn at_shop(&self, unit_id: EntityId) -> bool {
        self.units.get(unit_id).is_some_and(|u| {
            u.pos.within(
                crate::sim::fountain_pos(u.team),
                rules::units(rules::SHOP_RANGE),
            )
        })
    }

    /// Whether a seat may buy this item right now.
    pub fn validate_buy(
        &self,
        slot: SlotId,
        unit_id: EntityId,
        item: ItemId,
    ) -> Result<(), RejectReason> {
        let Some(def) = rules::ITEMS.get(usize::from(item.0)) else {
            return Err(RejectReason::UnknownItem);
        };
        let seat = self.seat(slot).expect("validated live");
        if seat.gold < def.cost {
            return Err(RejectReason::NotEnoughGold);
        }
        let free = if self.at_shop(unit_id) {
            seat.items.iter().any(|s| s.is_none())
        } else {
            seat.items
                .iter()
                .enumerate()
                .any(|(i, s)| in_stash(i) && s.is_none())
        };
        if !free {
            return Err(RejectReason::InventoryFull);
        }
        Ok(())
    }

    /// Whether a seat may use the item in this slot right now.
    pub fn validate_use(&self, slot: SlotId, target_slot: ItemSlot) -> Result<(), RejectReason> {
        let seat = self.seat(slot).expect("validated live");
        let idx = usize::from(target_slot.0);
        let Some(Some(stack)) = seat.items.get(idx) else {
            return Err(RejectReason::EmptySlot);
        };
        if !in_inventory(idx) {
            return Err(RejectReason::Disabled);
        }
        if stack.cooldown > 0 {
            return Err(RejectReason::OnCooldown);
        }
        if rules::ITEMS[usize::from(stack.id.0)].charges == 0 {
            return Err(RejectReason::WrongTargetKind);
        }
        Ok(())
    }

    /// Whether a seat may sell the item in this slot right now.
    pub fn validate_sell(
        &self,
        slot: SlotId,
        unit_id: EntityId,
        target_slot: ItemSlot,
    ) -> Result<(), RejectReason> {
        let seat = self.seat(slot).expect("validated live");
        let Some(Some(_)) = seat.items.get(usize::from(target_slot.0)) else {
            return Err(RejectReason::EmptySlot);
        };
        if !self.at_shop(unit_id) {
            return Err(RejectReason::NotAtShop);
        }
        Ok(())
    }

    /// Whether a seat may move an item between these two slots right now.
    pub fn validate_move(
        &self,
        slot: SlotId,
        unit_id: EntityId,
        from: ItemSlot,
        to: ItemSlot,
    ) -> Result<(), RejectReason> {
        let seat = self.seat(slot).expect("validated live");
        let (from, to) = (usize::from(from.0), usize::from(to.0));
        if from >= TOTAL_SLOTS || to >= TOTAL_SLOTS || from == to {
            return Err(RejectReason::EmptySlot);
        }
        if seat.items[from].is_none() {
            return Err(RejectReason::EmptySlot);
        }
        if (in_stash(from) || in_stash(to)) && !self.at_shop(unit_id) {
            return Err(RejectReason::NotAtShop);
        }
        Ok(())
    }

    /// Buys an item: gold out, the stack into the first free slot — of
    /// everything at the shop, of the stash alone elsewhere.
    pub fn apply_buy(&mut self, slot: SlotId, unit_id: EntityId, item: ItemId) {
        if self.validate_buy(slot, unit_id, item).is_err() {
            return;
        }
        let at_shop = self.at_shop(unit_id);
        let tick = self.tick;
        let def = &rules::ITEMS[usize::from(item.0)];
        let cost = def.cost;
        let charges = def.charges;
        let seat = self.seat_mut(slot).expect("validated live");
        seat.gold -= cost;
        let free = seat
            .items
            .iter()
            .enumerate()
            .position(|(i, s)| s.is_none() && (at_shop || in_stash(i)))
            .expect("validated a free slot");
        seat.items[free] = Some(ItemStack {
            id: item,
            charges,
            cooldown: 0,
            bought_tick: tick,
            touched: false,
        });
    }

    /// Sells an item: half price back, the full price inside the fresh
    /// unused window.
    pub fn apply_sell(&mut self, slot: SlotId, unit_id: EntityId, target_slot: ItemSlot) {
        if self.validate_sell(slot, unit_id, target_slot).is_err() {
            return;
        }
        let tick = self.tick;
        let seat = self.seat_mut(slot).expect("validated live");
        let stack = seat.items[usize::from(target_slot.0)]
            .take()
            .expect("validated occupied");
        let def = &rules::ITEMS[usize::from(stack.id.0)];
        let fresh =
            !stack.touched && tick.saturating_sub(stack.bought_tick) <= rules::SELL_REFUND_TICKS;
        let refund = if fresh {
            def.cost
        } else {
            def.cost * rules::SELL_PCT / 100
        };
        seat.gold += refund;
        seat.net_worth -= def.cost - refund;
    }

    /// Moves an item, swapping with whatever sits in the way. A stack that
    /// leaves the backpack for the inventory is muted for the mute window
    /// before it works again.
    pub fn apply_move(&mut self, slot: SlotId, unit_id: EntityId, from: ItemSlot, to: ItemSlot) {
        if self.validate_move(slot, unit_id, from, to).is_err() {
            return;
        }
        let (from, to) = (usize::from(from.0), usize::from(to.0));
        let seat = self.seat_mut(slot).expect("validated live");
        seat.items.swap(from, to);
        // After the swap the stack from `from` sits at `to` and vice versa.
        for (origin, landed) in [(from, to), (to, from)] {
            if let Some(stack) = &mut seat.items[landed] {
                stack.touched = true;
                if in_backpack(origin) && in_inventory(landed) {
                    stack.cooldown = stack.cooldown.max(rules::BACKPACK_MUTE_TICKS);
                }
            }
        }
    }

    /// Uses a consumable: the regeneration starts, a charge burns, an empty
    /// stack vanishes.
    pub fn apply_use(&mut self, slot: SlotId, unit_id: EntityId, target_slot: ItemSlot) {
        if self.validate_use(slot, target_slot).is_err() {
            return;
        }
        let seat = self.seat_mut(slot).expect("validated live");
        let idx = usize::from(target_slot.0);
        let stack = seat.items[idx].as_mut().expect("validated occupied");
        let id = stack.id.0;
        stack.touched = true;
        stack.charges = stack.charges.saturating_sub(1);
        if stack.charges == 0 {
            seat.items[idx] = None;
        }
        if let Some(unit) = self.units.get_mut(unit_id) {
            match id {
                rules::ITEM_SALVE => unit.salve_ticks = rules::REGEN_BUFF_TICKS,
                rules::ITEM_CLARITY => unit.clarity_ticks = rules::REGEN_BUFF_TICKS,
                _ => {}
            }
        }
    }

    /// Ticks item cooldowns, drinks pulses, and keeps every hero's stats in
    /// step with what works in its inventory.
    pub fn tick_items(&mut self) {
        for seat in &mut self.seats {
            for stack in seat.items.iter_mut().flatten() {
                stack.cooldown = stack.cooldown.saturating_sub(1);
            }
        }
        for i in 0..self.seats.len() {
            let Some(unit_id) = self.seats[i].unit else {
                continue;
            };
            let wanted = carried_bonus(&self.seats[i].items);
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            let applied = unit.item_bonus;
            if wanted != applied {
                unit.move_speed += rules::units(wanted.move_speed - applied.move_speed);
                unit.attack_damage += wanted.damage - applied.damage;
                unit.armor += wanted.armor - applied.armor;
                grow_pool(&mut unit.hp, &mut unit.max_hp, wanted.hp - applied.hp);
                grow_pool(
                    &mut unit.mana,
                    &mut unit.max_mana,
                    wanted.mana - applied.mana,
                );
                unit.item_bonus = wanted;
            }
            // The drunk regeneration pulses on their own clocks.
            if unit.salve_ticks > 0 {
                unit.salve_ticks -= 1;
                if unit.salve_ticks % rules::SALVE_PULSE_TICKS == 0 {
                    unit.hp = (unit.hp + rules::SALVE_HP_PER_PULSE).min(unit.max_hp);
                }
            }
            if unit.clarity_ticks > 0 {
                unit.clarity_ticks -= 1;
                if unit.clarity_ticks % rules::CLARITY_PULSE_TICKS == 0 {
                    unit.mana = (unit.mana + rules::CLARITY_MANA_PER_PULSE).min(unit.max_mana);
                }
            }
        }
    }

    /// Breaks the drunk regeneration of a unit hit by a hero.
    pub fn dispel_regen(&mut self, target: EntityId, source: Option<EntityId>) {
        let hero_hit = source
            .and_then(|s| self.units.get(s))
            .is_some_and(|s| s.kind == UnitKind::Hero);
        if !hero_hit {
            return;
        }
        if let Some(unit) = self.units.get_mut(target) {
            unit.salve_ticks = 0;
            unit.clarity_ticks = 0;
        }
    }
}

/// Changes a pool's maximum by `delta`, keeping the filled fraction.
fn grow_pool(current: &mut i32, max: &mut i32, delta: i32) {
    if delta == 0 || *max <= 0 {
        *max += delta;
        return;
    }
    let new_max = (*max + delta).max(1);
    *current = (i64::from(*current) * i64::from(new_max) / i64::from(*max)).max(1) as i32;
    *max = new_max;
}

/// The wire view of a slice of item slots.
pub fn item_views(items: &[Option<ItemStack>]) -> Vec<Option<ItemView>> {
    items
        .iter()
        .map(|s| {
            s.map(|stack| ItemView {
                id: stack.id,
                charges: stack.charges,
                cooldown_left: stack.cooldown,
            })
        })
        .collect()
}
