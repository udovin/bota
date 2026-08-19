//! Whole-tick behavior: orders, combat, economy, victory.

use bota_proto::{DamageKind, EventKind, Fixed, Order, RejectReason, SlotId, Team, UnitKind, Vec2};

use super::fixtures::{aim_along_lane, hero_id, world};
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
            pos: Vec2::from_ints(3200, 1176),
        },
    )]);
    step_n(&mut w, 29);
    let hero = w.units.get(hero_id(&w, 0)).unwrap();
    assert!(
        hero.pos.x.to_int() > 1400,
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
            pos: Vec2::from_ints(2160, 2558),
        },
    )]);
    step_n(&mut w, 20);
    let hero = w.units.get(hero_id(&w, 0)).unwrap();
    assert_eq!(hero.pos, Vec2::from_ints(2160, 2558));
    assert_eq!(hero.order, UnitOrder::Idle);
}

#[test]
fn creep_waves_spawn_on_schedule() {
    let mut w = world();
    step_n(&mut w, rules::FIRST_WAVE_TICK);
    let creeps = w.units.iter().filter(|(_, u)| u.is_creep()).count();
    assert_eq!(creeps, 24, "four creeps per lane per side");
}

#[test]
fn a_last_hit_pays_gold_and_counts() {
    let mut w = world();
    // Out at the river, beyond every fountain and tower, so the hero alone
    // gets the kill.
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8600, 8600)));
    w.units.get_mut(creep).unwrap().hp = 30;
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8300, 8300);
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
    // Nothing else stands in acquisition range, so the kill ends the order.
    w.step(&[]);
    let hero = w.units.get(hero_id(&w, 0)).unwrap();
    assert_eq!(hero.order, UnitOrder::Idle);
    assert_eq!(hero.engage, None);
}

#[test]
fn rest_starts_no_fight_however_it_was_reached() {
    let mut w = world();
    // Standing next to a wave, arriving next to it, or stopping next to it:
    // an idle hero attacks nothing on its own.
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8300, 8600);
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8400, 8600)));
    w.units.get_mut(creep).unwrap().move_speed = bota_proto::Fixed::ZERO;
    step_n(&mut w, 40);
    assert_eq!(
        w.units.get(hero).unwrap().engage,
        None,
        "no order, no fight"
    );
    w.step(&[cmd(
        0,
        Order::Move {
            pos: Vec2::from_ints(8300, 8800),
        },
    )]);
    step_n(&mut w, 30);
    assert_eq!(
        w.units.get(hero).unwrap().engage,
        None,
        "arriving somewhere is not an attack either"
    );
    w.step(&[cmd(0, Order::Stop)]);
    step_n(&mut w, 30);
    assert_eq!(w.units.get(hero).unwrap().engage, None, "neither is a stop");
}

#[test]
fn a_kill_rolls_the_attack_onto_the_next_creep() {
    let mut w = world();
    // The player attacked a creep: when it dies, the fight continues onto
    // the closest enemy in acquisition range by itself.
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8300, 8600);
    let first = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8500, 8600)));
    w.units.get_mut(first).unwrap().move_speed = bota_proto::Fixed::ZERO;
    w.units.get_mut(first).unwrap().hp = 30;
    let second = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8600, 8600)));
    w.units.get_mut(second).unwrap().move_speed = bota_proto::Fixed::ZERO;
    w.step(&[cmd(0, Order::AttackUnit { target: first })]);
    for _ in 0..300 {
        if !w.units.contains(first) {
            break;
        }
        w.step(&[]);
    }
    assert!(!w.units.contains(first), "the first creep dies");
    step_n(&mut w, 60);
    assert_eq!(
        w.units.get(hero).unwrap().engage,
        Some(second),
        "and the attack rolled onto the next one"
    );
    let c = w.units.get(second).unwrap();
    assert!(c.hp < c.max_hp, "with swings landing");
}

#[test]
fn denying_a_low_friendly_creep_counts_a_deny() {
    let mut w = world();
    // Out of the fountain's reach, so the deny is the hero's alone. Rooted,
    // or it would march down the lane faster than the hero walks.
    let creep = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(1900, 1900),
    ));
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
fn an_attack_order_at_a_healthy_ally_follows_without_harm() {
    let mut w = world();
    let creep = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(2500, 3000),
    ));
    w.units.get_mut(creep).unwrap().move_speed = bota_proto::Fixed::ZERO;
    assert_eq!(
        w.validate(SlotId(0), &Order::AttackUnit { target: creep }),
        Ok(())
    );
    w.step(&[cmd(0, Order::AttackUnit { target: creep })]);
    step_n(&mut w, 140);
    let ally = w.units.get(creep).unwrap();
    assert_eq!(ally.hp, ally.max_hp, "an ally is followed, not hit");
    let hero = w.units.get(hero_id(&w, 0)).unwrap();
    assert!(
        hero.pos.within(
            Vec2::from_ints(2500, 3000),
            rules::units(rules::FOLLOW_DISTANCE + 60)
        ),
        "the hero walked up to its ally, at {:?}",
        hero.pos
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
            &Order::UseItem {
                slot: bota_proto::ItemSlot(0),
                target: bota_proto::OrderTarget::None,
            }
        ),
        Err(RejectReason::EmptySlot)
    );
    // The shop answers, but not for an item outside the catalog.
    assert_eq!(
        w.validate(
            SlotId(0),
            &Order::BuyItem {
                item: bota_proto::ItemId(999)
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
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(12000, 12000);
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
fn creeps_attack_the_closest_enemy_regardless_of_kind() {
    let mut w = world();
    let dire_creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(4300, 4096)));
    let radiant_creep = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(4000, 4096),
    ));
    // The radiant hero stands closer to the dire creep than the radiant creep does.
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(4200, 4096);
    w.step(&[]);
    assert_eq!(w.units.get(dire_creep).unwrap().engage, Some(hero));
    let _ = radiant_creep;
}

#[test]
fn an_attack_order_on_a_hero_provokes_nearby_creeps() {
    let mut w = world();
    let radiant_hero = hero_id(&w, 0);
    let dire_hero = hero_id(&w, 1);
    w.units.get_mut(radiant_hero).unwrap().pos = Vec2::from_ints(8196, 8196);
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(8496, 8496);
    let guard = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8396, 8196)));
    // A closer creep target exists, so only provocation explains a switch.
    let radiant_creep = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8296, 8096),
    ));
    w.step(&[cmd(0, Order::AttackUnit { target: dire_hero })]);
    assert_eq!(w.units.get(guard).unwrap().engage, Some(radiant_hero));
    // The offender vanishes across the map. A hero is held only while it
    // stays in reach, so the guard goes back to the closest enemy it sees.
    w.units.get_mut(radiant_hero).unwrap().pos = Vec2::from_ints(12000, 9000);
    w.units.get_mut(radiant_hero).unwrap().move_speed = bota_proto::Fixed::ZERO;
    step_n(&mut w, 2);
    assert_eq!(w.units.get(guard).unwrap().engage, Some(radiant_creep));
}

#[test]
fn last_hitting_a_creep_provokes_no_one() {
    let mut w = world();
    let radiant_hero = hero_id(&w, 0);
    w.units.get_mut(radiant_hero).unwrap().pos = Vec2::from_ints(4100, 4100);
    let victim = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(4250, 4100)));
    let guard = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(4300, 4200)));
    let radiant_creep = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(4200, 4300),
    ));
    w.step(&[cmd(0, Order::AttackUnit { target: victim })]);
    assert_eq!(
        w.units.get(guard).unwrap().engage,
        Some(radiant_creep),
        "an order against a creep is not a call to arms"
    );
}

#[test]
fn movement_steers_around_a_friendly_tower() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(4600, 4600);
    // The straight line to the destination runs through the radiant mid
    // tier-two tower.
    w.step(&[cmd(
        0,
        Order::Move {
            pos: Vec2::from_ints(5900, 5900),
        },
    )]);
    step_n(&mut w, 260);
    let pos = w.units.get(hero).unwrap().pos;
    assert!(
        pos.within(Vec2::from_ints(5900, 5900), rules::units(60)),
        "went around and arrived, at {pos:?}"
    );
}

#[test]
fn a_swing_leaves_a_recovery_pause() {
    let mut w = world();
    let attacker = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(9100, 9000)));
    let victim = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(9160, 9000),
    ));
    w.units.get_mut(victim).unwrap().move_speed = bota_proto::Fixed::ZERO;
    for _ in 0..60 {
        w.step(&[]);
        if w.units.get(victim).unwrap().hp < rules::MELEE_CREEP_HP {
            break;
        }
    }
    let a = w.units.get(attacker).unwrap();
    assert!(a.recovering > 0, "the landed swing has a backswing");
    let stand = a.pos;
    // The victim vanishes mid-recovery; the attacker still gathers itself.
    w.units.remove(victim);
    w.step(&[]);
    assert_eq!(
        w.units.get(attacker).unwrap().pos,
        stand,
        "no marching off during the backswing"
    );
    step_n(&mut w, 60);
    assert_ne!(
        w.units.get(attacker).unwrap().pos,
        stand,
        "and then it moves on"
    );
}

#[test]
fn a_chase_into_the_towers_kills_the_chaser() {
    let mut w = world();
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(7600, 7600)));
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(7660, 7660);
    w.step(&[]);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(hero),
        "the closest enemy is acquired"
    );
    // The hero flees home. A lone chaser follows the closest-enemy rule right
    // under the Radiant towers, and that is where the chase ends: dragging a
    // creep to one's own fountain means dragging it through four tower tiers.
    w.step(&[cmd(
        0,
        Order::Move {
            pos: Vec2::from_ints(800, 800),
        },
    )]);
    step_n(&mut w, 600);
    assert!(
        !w.units.contains(creep),
        "the towers put the chase down on the way"
    );
    assert!(
        w.seat(SlotId(0)).unwrap().unit.is_some(),
        "the hero jogged home alive"
    );
}

#[test]
fn a_walking_body_ahead_costs_the_creep_distance() {
    // The same march twice, with and without a hero walking in front of it.
    // However the contact is resolved, the blocked run has to fall behind.
    let start = Vec2::from_ints(6940, 6940);
    let run = |block: bool| -> i64 {
        let mut w = world();
        let creep = w.units.insert(Unit::melee_creep(Team::Radiant, start));
        aim_along_lane(&mut w, creep, rules::LANE_MID);
        w.step(&[]);
        let UnitOrder::AttackMove { pos: heading } = w.units.get(creep).unwrap().order else {
            panic!("a lane creep marches its route");
        };
        let hero = hero_id(&w, 0);
        if block {
            w.units.get_mut(hero).unwrap().pos =
                crate::sim::move_towards(start, heading, rules::units(90));
            // Slowed well under creep pace, the walking body is a real block.
            w.units.get_mut(hero).unwrap().move_speed = rules::units(200);
            w.step(&[cmd(0, Order::Move { pos: heading })]);
        } else {
            w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(1200, 1200);
            w.step(&[]);
        }
        for _ in 0..380 {
            w.step(&[]);
        }
        w.units.get(creep).unwrap().pos.distance_squared(start)
    };
    let free = run(false);
    let blocked = run(true);
    assert!(
        blocked < free,
        "the block cost the creep ground: {blocked} against {free} unblocked"
    );
}

#[test]
fn a_wave_jammed_against_a_stander_flows_around_it() {
    let mut w = world();
    // A hero stops dead on the lane with a whole wave right behind: the
    // walkers stay intangible to each other even while blocked, so the jam
    // pours around the stander instead of freezing into a mutual wall.
    let hero = hero_id(&w, 0);
    let spot = Vec2::from_ints(8192, 8192);
    w.units.get_mut(hero).unwrap().pos = spot;
    let mut wave = Vec::new();
    for at in [
        Vec2::from_ints(8050, 8050),
        Vec2::from_ints(8060, 7990),
        Vec2::from_ints(7990, 8060),
        Vec2::from_ints(7980, 7980),
    ] {
        let creep = w.units.insert(Unit::melee_creep(Team::Radiant, at));
        aim_along_lane(&mut w, creep, rules::LANE_MID); // already past its own towers
        wave.push(creep);
    }
    for _ in 0..300 {
        w.step(&[]);
    }
    assert_eq!(w.units.get(hero).unwrap().pos, spot, "the stander held");
    for creep in wave {
        let pos = w.units.get(creep).unwrap().pos;
        assert!(
            pos.x + pos.y > spot.x + spot.y,
            "every creep got around and past, one is at {pos:?}"
        );
    }
}

#[test]
fn a_standing_hero_is_routed_around_untouched() {
    let mut w = world();
    // A hero stands still in the river on the mid lane. Standing units are
    // painted into the grid: the wave plans around the hero in advance,
    // never entering the circle and never displacing the stander.
    let hero = hero_id(&w, 0);
    let spot = Vec2::from_ints(8192, 8192);
    w.units.get_mut(hero).unwrap().pos = spot;
    let creep = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(7600, 7600),
    ));
    aim_along_lane(&mut w, creep, rules::LANE_MID); // already past its own towers
    let min = {
        let c = w.units.get(creep).unwrap();
        c.radius + w.units.get(hero).unwrap().radius
    };
    let mut entered = false;
    let mut bumped = false;
    for _ in 0..300 {
        w.step(&[]);
        let (Some(c), Some(h)) = (w.units.get(creep), w.units.get(hero)) else {
            break;
        };
        if c.pos.within(h.pos, min - Fixed::EPSILON) {
            entered = true;
        }
        if c.pos.within(h.pos, min + rules::units(15)) {
            bumped = true;
        }
    }
    assert!(!entered, "the stander is never entered");
    assert!(
        bumped,
        "the creep walks all the way into the bump, no swerving early"
    );
    assert_eq!(w.units.get(hero).unwrap().pos, spot, "and never displaced");
    let creep_pos = w.units.get(creep).unwrap().pos;
    assert!(
        creep_pos.x + creep_pos.y > spot.x + spot.y,
        "the creep went around and got past, at {creep_pos:?}"
    );
}

#[test]
fn side_lane_creeps_hug_their_lane_not_the_middle() {
    let mut w = world();
    step_n(&mut w, rules::FIRST_WAVE_TICK + 300);
    let top_positions: Vec<Vec2> = w
        .units
        .iter()
        .filter(|(_, u)| u.is_creep() && u.team == Team::Radiant && u.lane == rules::LANE_TOP)
        .map(|(_, u)| u.pos)
        .collect();
    assert!(!top_positions.is_empty(), "the top wave exists");
    for pos in top_positions {
        assert!(
            crate::sim::lane_offset_squared(w.map, rules::LANE_TOP, pos)
                < rules::units(300).squared_raw(),
            "marching the west edge, at {pos:?}"
        );
        assert!(
            pos.y.to_int() > 2200,
            "made progress up the lane, at {pos:?}"
        );
    }
}

#[test]
fn creeps_walk_past_their_own_tower() {
    let mut w = world();
    let spawn = rules::RADIANT_CREEP_SPAWNS[usize::from(rules::LANE_MID)];
    let ids: Vec<_> = (0..3)
        .map(|i| {
            w.units.insert(Unit::melee_creep(
                Team::Radiant,
                spawn + Vec2::from_ints(i * 48 - 48, 0),
            ))
        })
        .collect();
    step_n(&mut w, 400);
    for id in ids {
        let pos = w.units.get(id).unwrap().pos;
        assert!(
            pos.x.to_int() > 2600,
            "marched past the tower rather than jamming on it, at {pos:?}"
        );
    }
}

#[test]
fn a_tower_takes_the_closest_and_keeps_it() {
    let mut w = world();
    let tower = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::Tower && u.pos == Vec2::from_ints(6026, 6290))
        .map(|(id, _)| id)
        .unwrap();
    let hero = hero_id(&w, 1);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(6122, 6286);
    w.step(&[]);
    assert_eq!(
        w.units.get(tower).unwrap().engage,
        Some(hero),
        "the closest enemy in reach is taken"
    );
    // A creep arriving later, even closer, does not steal the shot.
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(6072, 6306)));
    w.step(&[]);
    assert_eq!(w.units.get(tower).unwrap().engage, Some(hero));
    let _ = creep;
}

#[test]
fn a_tower_turns_on_a_hero_who_attacks_a_hero_in_its_reach() {
    let mut w = world();
    let tower = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::Tower && u.pos == Vec2::from_ints(11712, 11328))
        .map(|(id, _)| id)
        .unwrap();
    let radiant_hero = hero_id(&w, 0);
    let dire_hero = hero_id(&w, 1);
    w.units.get_mut(radiant_hero).unwrap().pos = Vec2::from_ints(11424, 11190);
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(11474, 11140);
    // The tower is already busy with a creep standing closer. The hero holds
    // an idle move so an auto-attack swing does not call the tower early.
    let creep = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(11524, 11328),
    ));
    w.step(&[cmd(
        0,
        Order::Move {
            pos: Vec2::from_ints(11424, 11190),
        },
    )]);
    assert_eq!(w.units.get(tower).unwrap().engage, Some(creep));
    w.step(&[cmd(0, Order::AttackUnit { target: dire_hero })]);
    assert_eq!(
        w.units.get(tower).unwrap().engage,
        Some(radiant_hero),
        "diving under a tower draws it"
    );
    // The dive escape: click an ally and the tower lets go too.
    w.step(&[cmd(0, Order::AttackUnit { target: creep })]);
    assert_ne!(
        w.units.get(tower).unwrap().engage,
        Some(radiant_hero),
        "the ally click sheds tower aggro"
    );
}

#[test]
fn an_order_at_an_ally_calls_the_creeps_off() {
    let mut w = world();
    // Out at the river, where no tower interferes, and nobody walks: the
    // question is only who the guard is aimed at.
    let radiant_hero = hero_id(&w, 0);
    let dire_hero = hero_id(&w, 1);
    w.units.get_mut(radiant_hero).unwrap().pos = Vec2::from_ints(8300, 8300);
    w.units.get_mut(radiant_hero).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(8600, 8600);
    let guard = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8500, 8300)));
    w.units.get_mut(guard).unwrap().move_speed = Fixed::ZERO;
    let ally = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8350, 8350),
    ));
    w.units.get_mut(ally).unwrap().move_speed = Fixed::ZERO;
    w.step(&[cmd(0, Order::AttackUnit { target: dire_hero })]);
    assert_eq!(w.units.get(guard).unwrap().engage, Some(radiant_hero));
    // The call answers once per cooldown, so the trick lands after it.
    step_n(&mut w, rules::ORDER_AGGRO_COOLDOWN_TICKS);
    w.step(&[cmd(0, Order::AttackUnit { target: ally })]);
    assert_ne!(
        w.units.get(guard).unwrap().engage,
        Some(radiant_hero),
        "clicking an ally puts the orderer last among equally close ones"
    );
    // And proximity alone does not bring it back: a held target is held.
    step_n(&mut w, 30);
    assert_ne!(w.units.get(guard).unwrap().engage, Some(radiant_hero));
}

#[test]
fn the_fountain_burns_intruders() {
    let mut w = world();
    let dire_hero = hero_id(&w, 1);
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(1500, 2100);
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
    // Out at the river, beyond every fountain and tower, so the kill belongs
    // to the hero.
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(8600, 8600);
    let radiant_hero = hero_id(&w, 0);
    w.units.get_mut(radiant_hero).unwrap().pos = Vec2::from_ints(8300, 8300);
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
    assert!(pos.within(rules::DIRE_FOUNTAIN_POS, rules::units(450)));
}

fn smite(w: &mut World, target: bota_proto::EntityId) {
    let mut events = Vec::new();
    let deaths = w.resolve_damage(
        vec![DamageInst {
            source: None,
            slot: Some(SlotId(0)),
            team: Team::Radiant,
            target,
            amount: 1_000_000,
            kind: DamageKind::Pure,
            crit: false,
        }],
        &mut events,
    );
    w.process_deaths(deaths, &mut events);
}

#[test]
fn the_ancient_opens_only_after_the_tier_fours() {
    let mut w = world();
    let ancient = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::Ancient && u.team == Team::Dire)
        .map(|(id, _)| id)
        .unwrap();
    // Sheltered by its tier-four towers, the Ancient shrugs everything off.
    smite(&mut w, ancient);
    assert!(w.units.contains(ancient), "invulnerable means invulnerable");
    assert_eq!(w.winner(), None);
    let tier_fours: Vec<_> = w
        .units
        .iter()
        .filter(|(_, u)| u.kind == UnitKind::Tower && u.team == Team::Dire && u.tier == 4)
        .map(|(id, _)| id)
        .collect();
    assert_eq!(
        tier_fours.len(),
        2,
        "two tier-four towers guard the Ancient"
    );
    for id in tier_fours {
        smite(&mut w, id);
    }
    assert!(
        !w.units.get(ancient).unwrap().invulnerable,
        "the shield fell with the last tier four"
    );
    smite(&mut w, ancient);
    assert_eq!(w.winner(), Some(Team::Radiant));
    assert_eq!(w.stats().slots.len(), 2);
}

#[test]
fn a_tower_kill_pays_its_bounty() {
    let mut w = world();
    let (tower, bounty) = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::Tower && u.team == Team::Dire)
        .map(|(id, u)| (id, u.bounty))
        .unwrap();
    let gold_before = w.seat(SlotId(0)).unwrap().gold;
    smite(&mut w, tower);
    assert_eq!(w.seat(SlotId(0)).unwrap().gold, gold_before + bounty);
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
                crit: false,
            },
            DamageInst {
                source: None,
                slot: None,
                team: Team::Dire,
                target: hero,
                amount: 100,
                kind: DamageKind::Magical,
                crit: false,
            },
            DamageInst {
                source: None,
                slot: None,
                team: Team::Dire,
                target: hero,
                amount: 100,
                kind: DamageKind::Pure,
                crit: false,
            },
        ],
        &mut events,
    );
    let hp_after = w.units.get(hero).unwrap().hp;
    // Armor 3: 100 * 100/118 = 84. Resist 25%: 75. Pure: 100.
    assert_eq!(hp_before - hp_after, 84 + 75 + 100);
    assert_eq!(events.len(), 3);
}
