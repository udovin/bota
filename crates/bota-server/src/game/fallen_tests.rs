//! A hero that falls keeps its body and its handle: it lies out of the
//! world, still answers to its own seat for the bag and the shop, and stands
//! up again as the same body.

use bota_proto::{
    AbilitySlot, DamageKind, Fixed, HeroId, ItemId, ItemSlot, MapId, Order, Pick, RejectReason,
    SlotId, Target, Team, TickMode, Vec2,
};

use crate::game::{Command, Entity, MatchConfig, World, wire_id};

const MINE: SlotId = SlotId(0);
const THEIRS: SlotId = SlotId(1);

/// Two seats on the demo map, with gold to spend.
fn two_seats() -> World {
    let cfg = MatchConfig {
        match_id: 11,
        master_key: [5; 32],
        picks: vec![
            Pick {
                slot: MINE,
                team: Team::Radiant,
                hero: HeroId(2),
            },
            Pick {
                slot: THEIRS,
                team: Team::Dire,
                hero: HeroId(0),
            },
        ],
        map: MapId(1),
        tick_rate: 30,
        mode: TickMode::Lockstep,
        ack_timeout_ticks: 30,
        rules: crate::game::STANDARD_RULES,
    };
    let mut world = World::for_match(&cfg, cfg.rng());
    for seat in &mut world.seats {
        seat.gold = 3000;
    }
    world
}

fn hero(world: &World, slot: SlotId) -> Entity {
    world
        .seats
        .iter()
        .find(|seat| seat.slot == slot)
        .and_then(|seat| seat.unit)
        .expect("a standing hero")
}

fn give(world: &mut World, slot: SlotId, unit: Entity, order: Order) {
    world.advance(&[Command {
        slot,
        unit: Some(wire_id(unit)),
        order,
    }]);
}

fn boots() -> Order {
    Order::Buy {
        item: ItemId(crate::game::ITEM_BOOTS),
    }
}

fn fall(world: &mut World, body: Entity) {
    world.bury(vec![(body, None)], &mut Vec::new());
}

/// Steps until the seat's hero stands again, with a few ticks after.
fn stand_up(world: &mut World, slot: SlotId) -> Entity {
    let index = world
        .seats
        .iter()
        .position(|seat| seat.slot == slot)
        .unwrap();
    world.seats[index].respawn_left = 1;
    for _ in 0..5 {
        world.step();
    }
    world.seats[index].unit.expect("it stood up")
}

#[test]
fn a_fallen_hero_keeps_its_handle_and_is_nobody_on_the_field() {
    let mut world = two_seats();
    let body = hero(&world, MINE);
    let handle = wire_id(body);
    fall(&mut world, body);

    assert!(world.entities.contains(body), "the body is kept");
    assert_eq!(world.seats[0].fallen, Some(body));
    assert_eq!(world.seats[0].unit, None, "but nothing stands");
    for team in [Team::Radiant, Team::Dire] {
        let view = world.view(team);
        assert!(
            view.units.iter().all(|unit| unit.id != handle),
            "{team:?} sees no body"
        );
        assert_eq!(
            view.players[0].unit,
            Some(handle),
            "{team:?} is told the handle all the same"
        );
    }
    assert!(
        world.view(Team::Radiant).players[0].kit.is_some(),
        "its side is told what it carries"
    );
    // Nobody can take it for a target, and it cannot be sent anywhere.
    assert_eq!(
        world.validate_order(
            THEIRS,
            None,
            &Order::Attack {
                target: Target::Unit(handle)
            }
        ),
        Err(RejectReason::UnknownTarget)
    );
    assert_eq!(
        world.validate_order(
            MINE,
            Some(handle),
            &Order::Move {
                target: Target::Pos(Vec2::from_ints(4000, 4000))
            }
        ),
        Err(RejectReason::HeroDead)
    );
    for table_has in [
        world.transform.get(body).is_some(),
        world.hull.get(body).is_some(),
        world.visibility.get(body).is_some(),
        world.action.get(body).is_some(),
        world.motion.get(body).is_some(),
    ] {
        assert!(!table_has, "it takes no part in the world");
    }

    let back = stand_up(&mut world, MINE);
    assert_eq!(back, body, "the same body stands up");
    assert_eq!(wire_id(back), handle, "under the same handle");
    let stats = *world.stats.get(back).unwrap();
    assert_eq!(world.health.get(back).unwrap().hp, stats.max_hp, "whole");
    assert!(world.at_shop(back), "at its fountain");
    assert!(
        world
            .view(Team::Radiant)
            .units
            .iter()
            .any(|u| u.id == handle),
        "and back in its side's view"
    );
}

#[test]
fn a_fallen_hero_buys_into_the_stash_and_marks_what_it_would_sell() {
    let mut world = two_seats();
    let body = hero(&world, MINE);
    give(&mut world, MINE, body, boots());
    assert_eq!(
        world.inventory.get(body).unwrap().slots[0].map(|s| s.id),
        Some(ItemId(crate::game::ITEM_BOOTS)),
        "bought at the fountain, the boots are carried"
    );
    fall(&mut world, body);
    let handle = wire_id(body);

    // Buying goes on: into the stash, the shop's to hold.
    assert_eq!(world.validate_order(MINE, Some(handle), &boots()), Ok(()));
    let gold = world.seats[0].gold;
    give(&mut world, MINE, body, boots());
    assert!(world.seats[0].gold < gold, "paid for");
    let stashed = world.seats[0]
        .stash
        .slots
        .iter()
        .position(|slot| slot.is_some_and(|s| s.id == ItemId(crate::game::ITEM_BOOTS)))
        .expect("the second boots wait in the stash");

    // Selling does not: it only marks, the carried boots and the stashed
    // ones alike, and no gold comes back.
    let before = world.seats[0].gold;
    give(&mut world, MINE, body, Order::Sell { slot: ItemSlot(0) });
    give(
        &mut world,
        MINE,
        body,
        Order::Sell {
            slot: ItemSlot((crate::game::BAG_SLOTS + stashed) as u8),
        },
    );
    assert_eq!(world.seats[0].gold, before, "nothing sold while it lies");
    assert!(
        world.inventory.get(body).unwrap().slots[0]
            .unwrap()
            .for_sale
    );
    assert!(world.seats[0].stash.slots[stashed].unwrap().for_sale);

    // Standing up at the fountain sells what was marked.
    let back = stand_up(&mut world, MINE);
    assert!(world.seats[0].gold > before, "the marked boots are sold");
    assert!(
        world.inventory.get(back).unwrap().slots[0].is_none(),
        "out of the bag"
    );
    assert!(
        world.seats[0].stash.slots[stashed].is_none(),
        "and out of the stash"
    );
}

#[test]
fn a_fallen_hero_learns_but_mends_no_mana_and_its_cooldowns_run_down() {
    let mut world = two_seats();
    let body = hero(&world, MINE);
    world.mana.get_mut(body).unwrap().mana = Fixed::from_int(10);
    world.abilities.get_mut(body).unwrap().slots[0].cooldown = 100;
    fall(&mut world, body);
    for _ in 0..40 {
        world.step();
    }
    assert_eq!(
        world.mana.get(body).unwrap().mana,
        Fixed::from_int(10),
        "a fallen hero mends nothing"
    );
    assert_eq!(
        world.abilities.get(body).unwrap().slots[0].cooldown,
        60,
        "but what it waits on runs down while it lies"
    );
    let points = |world: &World| {
        world
            .abilities
            .get(body)
            .unwrap()
            .slots
            .iter()
            .map(|s| u32::from(s.level))
            .sum::<u32>()
    };
    let learned = points(&world);
    give(
        &mut world,
        MINE,
        body,
        Order::Learn {
            slot: AbilitySlot(0),
        },
    );
    // Shadowraze's three slots learn together, so one point shows thrice.
    assert!(points(&world) > learned, "and it learns while it lies");
}

#[test]
fn two_killing_blows_on_one_tick_lay_a_hero_down_once() {
    let mut world = two_seats();
    let body = hero(&world, MINE);
    world.push_hit(None, body, 30_000, DamageKind::Pure);
    world.push_hit(None, body, 30_000, DamageKind::Pure);
    world.step();
    assert_eq!(world.seats[0].deaths, 1, "one death for one fall");
    assert_eq!(world.seats[0].fallen, Some(body));
}
