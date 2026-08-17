//! Whole-tick behavior: orders, combat, economy, victory.

use bota_proto::{DamageKind, EventKind, Order, RejectReason, SlotId, Team, UnitKind, Vec2};

use super::fixtures::{hero_id, world};
use crate::sim::{Command, DamageInst, Event, Unit, UnitOrder, World, rules};

fn step_n(w: &mut World, n: u32) -> Vec<Event> {
    let mut all = Vec::new();
    for _ in 0..n {
        all.extend(w.step(&[]));
    }
    all
}

fn cmd(slot: u8, order: Order) -> Command {
    Command {
        slot: SlotId(slot),
        order,
    }
}

#[test]
fn a_move_order_walks_the_hero() {
    let mut w = world();
    w.step(&[cmd(
        0,
        Order::Move {
            pos: Vec2::from_ints(2000, 256),
        },
    )]);
    step_n(&mut w, 29);
    let hero = w.units.get(hero_id(&w, 0)).unwrap();
    assert!(
        hero.pos.x.to_int() > 500,
        "a second of walking covers ground, at {:?}",
        hero.pos
    );
}

#[test]
fn arrival_ends_a_move_order() {
    let mut w = world();
    w.step(&[cmd(
        0,
        Order::Move {
            pos: Vec2::from_ints(500, 396),
        },
    )]);
    step_n(&mut w, 20);
    let hero = w.units.get(hero_id(&w, 0)).unwrap();
    assert_eq!(hero.pos, Vec2::from_ints(500, 396));
    assert_eq!(hero.order, UnitOrder::Idle);
}

#[test]
fn creep_waves_spawn_on_schedule() {
    let mut w = world();
    step_n(&mut w, rules::FIRST_WAVE_TICK);
    let creeps = w.units.iter().filter(|(_, u)| u.is_creep()).count();
    assert_eq!(creeps, 8, "three melee and one ranged per side");
}

#[test]
fn the_fifth_wave_brings_a_siege_creep() {
    let mut w = world();
    w.tick = rules::FIRST_WAVE_TICK + 4 * rules::WAVE_PERIOD_TICKS;
    w.spawn_waves();
    let sieges = w
        .units
        .iter()
        .filter(|(_, u)| u.kind == UnitKind::CreepSiege)
        .count();
    assert_eq!(sieges, 2);
}

#[test]
fn a_last_hit_pays_gold_and_counts() {
    let mut w = world();
    // Out of the fountain's reach, so the hero gets the kill.
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(950, 950)));
    w.units.get_mut(creep).unwrap().hp = 30;
    let worth_before = w.seat(SlotId(0)).unwrap().net_worth;
    w.step(&[cmd(0, Order::AttackUnit { target: creep })]);
    for _ in 0..200 {
        if !w.units.contains(creep) {
            break;
        }
        w.step(&[]);
    }
    assert!(!w.units.contains(creep), "the creep must die");
    let seat = w.seat(SlotId(0)).unwrap();
    assert_eq!(seat.last_hits, 1);
    assert!(
        seat.net_worth >= worth_before + rules::MELEE_CREEP_BOUNTY,
        "bounty arrived"
    );
    assert!(seat.xp > 0, "creep experience arrived");
}

#[test]
fn denying_a_low_friendly_creep_counts_a_deny() {
    let mut w = world();
    // Out of the fountain's reach, so the deny is the hero's alone. Rooted,
    // or it would march down the lane faster than the hero walks.
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Radiant, Vec2::from_ints(950, 950)));
    w.units.get_mut(creep).unwrap().hp = 30;
    w.units.get_mut(creep).unwrap().move_speed = bota_proto::Fixed::ZERO;
    assert_eq!(
        w.validate(SlotId(0), &Order::AttackUnit { target: creep }),
        Ok(())
    );
    let worth_before = w.seat(SlotId(0)).unwrap().net_worth;
    w.step(&[cmd(0, Order::AttackUnit { target: creep })]);
    for _ in 0..200 {
        if !w.units.contains(creep) {
            break;
        }
        w.step(&[]);
    }
    let seat = w.seat(SlotId(0)).unwrap();
    assert_eq!(seat.denies, 1);
    assert_eq!(seat.last_hits, 0);
    // Passive gold still trickles, but no bounty.
    assert!(seat.net_worth < worth_before + rules::MELEE_CREEP_BOUNTY);
}

#[test]
fn a_healthy_friendly_creep_cannot_be_denied() {
    let mut w = world();
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Radiant, Vec2::from_ints(600, 600)));
    assert_eq!(
        w.validate(SlotId(0), &Order::AttackUnit { target: creep }),
        Err(RejectReason::CannotDeny)
    );
}

#[test]
fn a_fogged_target_is_unknown() {
    let w = world();
    let dire_hero = hero_id(&w, 1);
    assert_eq!(
        w.validate(SlotId(0), &Order::AttackUnit { target: dire_hero }),
        Err(RejectReason::UnknownTarget)
    );
}

#[test]
fn a_dead_entity_is_unknown_the_same_way() {
    let mut w = world();
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(600, 600)));
    w.units.remove(creep);
    assert_eq!(
        w.validate(SlotId(0), &Order::AttackUnit { target: creep }),
        Err(RejectReason::UnknownTarget)
    );
}

#[test]
fn the_fountain_cannot_be_attacked() {
    let w = world();
    let fountain = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::Fountain && u.team == Team::Radiant)
        .map(|(id, _)| id)
        .unwrap();
    assert_eq!(
        w.validate(SlotId(0), &Order::AttackUnit { target: fountain }),
        Err(RejectReason::WrongTargetKind)
    );
}

#[test]
fn unbuilt_systems_reject_their_orders() {
    let w = world();
    assert_eq!(
        w.validate(
            SlotId(0),
            &Order::CastAbility {
                slot: bota_proto::AbilitySlot(0),
                target: bota_proto::OrderTarget::None,
            }
        ),
        Err(RejectReason::EmptySlot)
    );
    assert_eq!(
        w.validate(
            SlotId(0),
            &Order::LevelUpAbility {
                slot: bota_proto::AbilitySlot(0)
            }
        ),
        Err(RejectReason::CannotLevelUp)
    );
    // At the fountain the shop answers; the catalog is empty.
    assert_eq!(
        w.validate(
            SlotId(0),
            &Order::BuyItem {
                item: bota_proto::ItemId(0)
            }
        ),
        Err(RejectReason::UnknownItem)
    );
}

#[test]
fn a_dead_hero_takes_no_orders() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    w.units.remove(hero);
    w.seats[0].unit = None;
    assert_eq!(
        w.validate(SlotId(0), &Order::Stop),
        Err(RejectReason::HeroDead)
    );
}

#[test]
fn an_attack_order_degrades_when_the_target_enters_fog() {
    let mut w = world();
    let dire_hero = hero_id(&w, 1);
    let near = Vec2::from_ints(700, 700);
    w.units.get_mut(dire_hero).unwrap().pos = near;
    w.step(&[cmd(0, Order::AttackUnit { target: dire_hero })]);
    // The target blinks across the map, out of every Radiant eye.
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(7900, 7900);
    w.step(&[]);
    let radiant = w.units.get(hero_id(&w, 0)).unwrap();
    match radiant.order {
        UnitOrder::AttackMove { pos } => {
            assert!(
                pos.within(near, rules::units(400)),
                "chases the last seen spot, not the real one: {pos:?}"
            );
        }
        ref other => panic!("expected degradation to AttackMove, found {other:?}"),
    }
    assert_eq!(radiant.engage, None);
}

#[test]
fn a_tower_prefers_creeps_over_heroes() {
    let mut w = world();
    let tower = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::Tower && u.team == Team::Radiant)
        .map(|(id, _)| id)
        .unwrap();
    let hero = hero_id(&w, 1);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(2400, 2300);
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(2500, 2400)));
    w.step(&[]);
    assert_eq!(w.units.get(tower).unwrap().engage, Some(creep));
}

#[test]
fn the_fountain_burns_intruders() {
    let mut w = world();
    let dire_hero = hero_id(&w, 1);
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(400, 400);
    step_n(&mut w, 60);
    // A missing unit means it burned down entirely, which also proves the point.
    if let Some(hp) = w.units.get(dire_hero).map(|u| u.hp) {
        assert!(hp < rules::HERO_HP, "the fountain hurt it: {hp}");
    }
}

#[test]
fn the_fountain_heals_its_own() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().hp = 100;
    step_n(&mut w, 10);
    let hp = w.units.get(hero).unwrap().hp;
    assert!(hp >= 100 + 9 * rules::FOUNTAIN_HEAL_HP_PER_TICK, "hp {hp}");
}

#[test]
fn a_hero_kill_pays_counts_and_respawns() {
    let mut w = world();
    let dire_hero = hero_id(&w, 1);
    // Out of the fountain's reach, so the kill belongs to the hero.
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(950, 950);
    w.units.get_mut(dire_hero).unwrap().hp = 1;
    // Keep the victim from walking home while the projectile flies.
    w.units.get_mut(dire_hero).unwrap().move_speed = bota_proto::Fixed::ZERO;
    let gold_before = w.seat(SlotId(0)).unwrap().gold;
    w.step(&[cmd(0, Order::AttackUnit { target: dire_hero })]);
    let mut died = Vec::new();
    for _ in 0..300 {
        died.extend(w.step(&[]));
        if w.seat(SlotId(1)).unwrap().unit.is_none() {
            break;
        }
    }
    let killer = w.seat(SlotId(0)).unwrap();
    let victim = w.seat(SlotId(1)).unwrap();
    assert_eq!(killer.kills, 1);
    assert_eq!(victim.deaths, 1);
    assert_eq!(victim.unit, None);
    assert!(victim.respawn_left > 0);
    assert!(
        killer.gold >= gold_before + rules::HERO_KILL_BOUNTY_BASE,
        "bounty paid"
    );
    assert!(
        died.iter().any(|e| matches!(
            e.kind,
            EventKind::Died {
                unit,
                killer: Some(_),
                denied: false,
            } if unit == dire_hero
        )),
        "a death event went out"
    );
    let wait = victim.respawn_left;
    step_n(&mut w, wait);
    let victim = w.seat(SlotId(1)).unwrap();
    let unit = victim.unit.expect("respawned");
    let pos = w.units.get(unit).unwrap().pos;
    assert!(pos.within(rules::DIRE_FOUNTAIN_POS, rules::units(200)));
}

#[test]
fn the_ancient_falling_ends_the_match() {
    let mut w = world();
    let ancient = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::Ancient && u.team == Team::Dire)
        .map(|(id, _)| id)
        .unwrap();
    let mut events = Vec::new();
    let deaths = w.resolve_damage(
        vec![DamageInst {
            source: None,
            slot: Some(SlotId(0)),
            team: Team::Radiant,
            target: ancient,
            amount: 1_000_000,
            kind: DamageKind::Pure,
        }],
        &mut events,
    );
    w.process_deaths(deaths, &mut events);
    assert_eq!(w.winner(), Some(Team::Radiant));
    assert!(events.iter().any(|e| matches!(
        e.kind,
        EventKind::StructureDestroyed {
            team: Team::Dire,
            ..
        }
    )));
    assert_eq!(w.stats().slots.len(), 2);
}

#[test]
fn a_tower_kill_pays_its_bounty() {
    let mut w = world();
    let tower = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::Tower && u.team == Team::Dire)
        .map(|(id, _)| id)
        .unwrap();
    let gold_before = w.seat(SlotId(0)).unwrap().gold;
    let mut events = Vec::new();
    let deaths = w.resolve_damage(
        vec![DamageInst {
            source: None,
            slot: Some(SlotId(0)),
            team: Team::Radiant,
            target: tower,
            amount: 1_000_000,
            kind: DamageKind::Pure,
        }],
        &mut events,
    );
    w.process_deaths(deaths, &mut events);
    assert_eq!(
        w.seat(SlotId(0)).unwrap().gold,
        gold_before + rules::TOWER_BOUNTY
    );
}

#[test]
fn experience_levels_the_hero_up() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    let mut events = Vec::new();
    w.grant_xp(0, rules::XP_THRESHOLDS[1], &mut events);
    let seat = w.seat(SlotId(0)).unwrap();
    assert_eq!(seat.level, 2);
    let unit = w.units.get(hero).unwrap();
    assert_eq!(unit.level, 2);
    assert_eq!(unit.max_hp, rules::HERO_HP + rules::HERO_HP_PER_LEVEL);
    assert_eq!(
        unit.attack_damage,
        rules::HERO_ATTACK_DAMAGE + rules::HERO_ATTACK_DAMAGE_PER_LEVEL
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e.kind, EventKind::LevelUp { level: 2, .. }))
    );
}

#[test]
fn armor_and_resist_shave_damage() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    let hp_before = w.units.get(hero).unwrap().hp;
    let mut events = Vec::new();
    w.resolve_damage(
        vec![
            DamageInst {
                source: None,
                slot: None,
                team: Team::Dire,
                target: hero,
                amount: 100,
                kind: DamageKind::Physical,
            },
            DamageInst {
                source: None,
                slot: None,
                team: Team::Dire,
                target: hero,
                amount: 100,
                kind: DamageKind::Magical,
            },
            DamageInst {
                source: None,
                slot: None,
                team: Team::Dire,
                target: hero,
                amount: 100,
                kind: DamageKind::Pure,
            },
        ],
        &mut events,
    );
    let hp_after = w.units.get(hero).unwrap().hp;
    // Armor 3: 100 * 100/118 = 84. Resist 25%: 75. Pure: 100.
    assert_eq!(hp_before - hp_after, 84 + 75 + 100);
    assert_eq!(events.len(), 3);
}
