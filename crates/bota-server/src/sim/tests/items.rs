//! The item engine: the shop, the stash, the backpack and carried bonuses.

use bota_proto::{DamageKind, ItemId, ItemSlot, Order, RejectReason, SlotId, Team, Vec2};

use super::fixtures::{hero_id, world};
use crate::sim::{Command, DamageInst, World, rules};

fn cmd(slot: u8, order: Order) -> Command {
    Command {
        slot: SlotId(slot),
        order,
    }
}

fn buy(w: &mut World, item: u16) {
    w.step(&[cmd(0, Order::BuyItem { item: ItemId(item) })]);
}

fn move_item(w: &mut World, from: u8, to: u8) {
    w.step(&[cmd(
        0,
        Order::MoveItem {
            from: ItemSlot(from),
            to: ItemSlot(to),
        },
    )]);
}

#[test]
fn buying_fills_the_inventory_at_the_shop_and_the_stash_elsewhere() {
    let mut w = world();
    let gold_before = w.seats[0].gold;
    buy(&mut w, 0);
    assert_eq!(w.seats[0].gold, gold_before - rules::ITEMS[0].cost);
    assert_eq!(
        w.seats[0].items[0].map(|s| s.id),
        Some(ItemId(0)),
        "bought at the shop, straight into the inventory"
    );
    // Out in the river the purchase lands in the stash instead.
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8300, 8600);
    w.seats[0].gold = 1000;
    buy(&mut w, 1);
    assert!(w.seats[0].items[1].is_none(), "nothing next to the boots");
    assert_eq!(
        w.seats[0].items[9].map(|s| s.id),
        Some(ItemId(1)),
        "bought remotely, into the stash"
    );
}

#[test]
fn carried_bonuses_work_only_in_the_inventory() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    let base = w.units.get(hero).unwrap().move_speed;
    buy(&mut w, 0);
    w.step(&[]);
    assert_eq!(
        w.units.get(hero).unwrap().move_speed,
        base + rules::units(45),
        "boots carried in the inventory work"
    );
    // Into the backpack: the bonus goes away at once, no cooldown involved.
    move_item(&mut w, 0, 6);
    w.step(&[]);
    assert_eq!(w.units.get(hero).unwrap().move_speed, base);
    for _ in 0..rules::BACKPACK_MUTE_TICKS * 2 {
        w.step(&[]);
    }
    assert_eq!(
        w.units.get(hero).unwrap().move_speed,
        base,
        "a backpack item never works, however long it sits there"
    );
    // Back out: leaving the backpack is what starts the mute window.
    move_item(&mut w, 6, 0);
    w.step(&[]);
    assert_eq!(
        w.units.get(hero).unwrap().move_speed,
        base,
        "muted right after coming back to the inventory"
    );
    for _ in 0..rules::BACKPACK_MUTE_TICKS {
        w.step(&[]);
    }
    assert_eq!(
        w.units.get(hero).unwrap().move_speed,
        base + rules::units(45),
        "and working once the mute runs out"
    );
}

#[test]
fn a_fresh_swap_into_the_inventory_stays_muted_for_the_window() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    let base = w.units.get(hero).unwrap().move_speed;
    buy(&mut w, 0);
    move_item(&mut w, 0, 6);
    move_item(&mut w, 6, 0);
    w.step(&[]);
    assert_eq!(
        w.units.get(hero).unwrap().move_speed,
        base,
        "still muted right after the swap back"
    );
    for _ in 0..rules::BACKPACK_MUTE_TICKS {
        w.step(&[]);
    }
    assert_eq!(
        w.units.get(hero).unwrap().move_speed,
        base + rules::units(45),
        "the mute runs out and the boots work again"
    );
}

#[test]
fn the_stash_opens_only_at_the_home_shop() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8300, 8600);
    buy(&mut w, 1); // lands in the stash remotely
    assert_eq!(
        w.validate(
            SlotId(0),
            &Order::MoveItem {
                from: ItemSlot(9),
                to: ItemSlot(0),
            }
        ),
        Err(RejectReason::NotAtShop)
    );
    // Walking home opens it.
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(2000, 2500);
    move_item(&mut w, 9, 0);
    assert_eq!(w.seats[0].items[0].map(|s| s.id), Some(ItemId(1)));
}

#[test]
fn selling_fresh_refunds_in_full_and_stale_in_half() {
    let mut w = world();
    let gold_before = w.seats[0].gold;
    buy(&mut w, 0);
    w.step(&[cmd(0, Order::SellItem { slot: ItemSlot(0) })]);
    assert_eq!(
        w.seats[0].gold, gold_before,
        "an unused fresh purchase refunds in full"
    );
    buy(&mut w, 0);
    let gold_after_buy = w.seats[0].gold;
    for _ in 0..rules::SELL_REFUND_TICKS + 1 {
        w.step(&[]);
    }
    w.step(&[cmd(0, Order::SellItem { slot: ItemSlot(0) })]);
    assert_eq!(
        w.seats[0].gold,
        gold_after_buy + rules::ITEMS[0].cost * rules::SELL_PCT / 100,
        "a stale one returns half"
    );
}

#[test]
fn the_salve_drips_health_until_a_hero_hit_spills_it() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    buy(&mut w, rules::ITEM_SALVE);
    w.units.get_mut(hero).unwrap().hp = 100;
    w.step(&[cmd(
        0,
        Order::UseItem {
            slot: ItemSlot(0),
            target: bota_proto::OrderTarget::None,
        },
    )]);
    assert!(w.seats[0].items[0].is_none(), "the last charge is drunk");
    let hp_at_use = w.units.get(hero).unwrap().hp;
    for _ in 0..90 {
        w.step(&[]);
    }
    let healed = w.units.get(hero).unwrap().hp - hp_at_use;
    assert!(
        healed >= 36,
        "ninety ticks of salve bring some forty health, got {healed}"
    );
    // A hero's hit spills the rest.
    let dire = hero_id(&w, 1);
    let mut events = Vec::new();
    let deaths = w.resolve_damage(
        vec![DamageInst {
            source: Some(dire),
            slot: Some(SlotId(1)),
            team: Team::Dire,
            target: hero,
            amount: 10,
            kind: DamageKind::Physical,
            crit: false,
        }],
        &mut events,
    );
    assert!(deaths.is_empty());
    assert_eq!(w.units.get(hero).unwrap().salve_ticks, 0, "spilled");
}

#[test]
fn a_consumable_cannot_be_used_from_the_backpack() {
    let mut w = world();
    buy(&mut w, rules::ITEM_SALVE);
    move_item(&mut w, 0, 6);
    assert_eq!(
        w.validate(
            SlotId(0),
            &Order::UseItem {
                slot: ItemSlot(6),
                target: bota_proto::OrderTarget::None,
            }
        ),
        Err(RejectReason::Disabled)
    );
}

#[test]
fn boosters_grow_the_pools_keeping_the_fraction() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    // Fill up first, then take the exact half before buying.
    let max = w.units.get(hero).unwrap().max_hp;
    w.units.get_mut(hero).unwrap().hp = max / 2;
    w.seats[0].gold = 2000;
    buy(&mut w, 5);
    w.step(&[]);
    let u = w.units.get(hero).unwrap();
    assert_eq!(u.max_hp, max + 250);
    assert!(
        (u.hp - u.max_hp / 2).abs() <= u.max_hp / 10,
        "the filled fraction survives the growth: {}/{}",
        u.hp,
        u.max_hp
    );
}
