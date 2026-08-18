//! Trees and the neutral camps.

use bota_proto::{Order, SlotId, UnitKind, Vec2};

use super::fixtures::{hero_id, world};
use crate::sim::{Command, Event, World, rules};

fn cmd(slot: u8, order: Order) -> Command {
    Command {
        slot: SlotId(slot),
        order,
    }
}

fn step_n(w: &mut World, n: u32) -> Vec<Event> {
    let mut all = Vec::new();
    for _ in 0..n {
        all.extend(w.step(&[]));
    }
    all
}

/// Steps until the first neutral spawn mark has just passed.
fn step_to_first_camps(w: &mut World) {
    while w.tick < rules::FIRST_NEUTRAL_TICK {
        w.step(&[]);
    }
}

fn neutrals_of(w: &World, camp: Vec2) -> usize {
    w.units
        .iter()
        .filter(|(_, u)| u.kind == UnitKind::CreepNeutral && u.camp == camp)
        .count()
}

#[test]
fn trees_block_the_ground_they_stand_on() {
    let w = world();
    let trees = crate::sim::tree_positions();
    assert!(trees.len() > 400, "the forest is a forest: {}", trees.len());
    let tree = trees[0];
    assert!(!w.grid.walkable(tree), "a tree trunk closes its cell");
}

#[test]
fn a_walk_ordered_into_a_tree_stops_at_the_trunk() {
    let mut w = world();
    // The trunk nearest the mid lane road, so the hero can reach its edge.
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(5856, 5856);
    let target = crate::sim::tree_positions()
        .into_iter()
        .min_by_key(|t| t.distance_squared(Vec2::from_ints(5856, 5856)))
        .expect("the forest exists");
    w.step(&[cmd(0, Order::Move { pos: target })]);
    for _ in 0..300 {
        w.step(&[]);
        let pos = w.units.get(hero).unwrap().pos;
        assert!(
            w.grid.walkable(pos),
            "the hero never stands inside the forest, at {pos:?}"
        );
    }
}

#[test]
fn the_lanes_and_camps_stay_clear_of_the_forest() {
    // Sampled lane midpoints between towers, and every camp clearing:
    // marching and fighting ground carries no trees.
    let w = world();
    let spots = [
        Vec2::from_ints(6849, 7049),  // mid, between tier two and tier one
        Vec2::from_ints(2797, 9708),  // top, between tier two and tier one
        Vec2::from_ints(11466, 2898), // bot, between tier two and tier one
        Vec2::from_ints(8706, 8838),  // the river crossing
    ];
    for spot in spots {
        assert!(w.grid.walkable(spot), "a lane is walkable at {spot:?}");
    }
    // The real spawner point can touch its clearing's treeline, so open
    // ground within a step of the camp center is enough.
    for camp in rules::NEUTRAL_CAMPS {
        let near = [
            camp,
            camp + Vec2::from_ints(150, 0),
            camp - Vec2::from_ints(150, 0),
            camp + Vec2::from_ints(0, 150),
            camp - Vec2::from_ints(0, 150),
        ];
        assert!(
            near.iter().any(|&p| w.grid.walkable(p)),
            "open ground by the camp at {camp:?}"
        );
    }
}

#[test]
fn camps_fill_on_the_minute_and_stay_full() {
    let mut w = world();
    let camp = rules::NEUTRAL_CAMPS[0];
    step_to_first_camps(&mut w);
    assert_eq!(
        neutrals_of(&w, camp),
        rules::NEUTRALS_PER_CAMP as usize,
        "the first minute stocks the camp"
    );
    assert_eq!(
        neutrals_of(&w, rules::NEUTRAL_CAMPS[2]),
        rules::NEUTRALS_PER_CAMP as usize,
        "the far-side camp too"
    );
    step_n(&mut w, rules::NEUTRAL_SPAWN_PERIOD_TICKS);
    assert_eq!(
        neutrals_of(&w, camp),
        rules::NEUTRALS_PER_CAMP as usize,
        "a full camp never double-stocks"
    );
}

#[test]
fn a_body_in_the_box_blocks_the_camp_spawn() {
    let mut w = world();
    let camp = rules::NEUTRAL_CAMPS[0];
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = camp;
    step_to_first_camps(&mut w);
    assert_eq!(neutrals_of(&w, camp), 0, "the stander blocked the spawn");
    // Everywhere else spawned fine.
    assert_eq!(
        neutrals_of(&w, rules::NEUTRAL_CAMPS[1]),
        rules::NEUTRALS_PER_CAMP as usize
    );
}

#[test]
fn neutrals_fight_back_leash_home_and_heal() {
    let mut w = world();
    let camp = rules::NEUTRAL_CAMPS[0];
    step_to_first_camps(&mut w);
    let neutral = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::CreepNeutral && u.camp == camp)
        .map(|(id, _)| id)
        .expect("the camp is stocked");
    // A hero walks in and attacks: the neutral answers.
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = camp + Vec2::from_ints(200, 0);
    w.step(&[cmd(0, Order::AttackUnit { target: neutral })]);
    let mut answered = false;
    for _ in 0..90 {
        w.step(&[]);
        if w.units.get(neutral).is_some_and(|n| n.engage == Some(hero)) {
            answered = true;
            break;
        }
    }
    assert!(answered, "the neutral turned on its attacker");
    // Kiting it beyond the leash sends it home, deaf, to a full heal.
    w.units.get_mut(hero).unwrap().pos = camp + Vec2::from_ints(rules::NEUTRAL_LEASH + 400, 0);
    let mut went_home = false;
    for _ in 0..600 {
        w.step(&[]);
        let Some(n) = w.units.get(neutral) else {
            break;
        };
        if !n.returning && n.engage.is_none() && n.pos.within(camp, rules::units(200)) {
            went_home = n.hp == n.max_hp;
            if went_home {
                break;
            }
        }
    }
    assert!(went_home, "leashed back to the camp at full health");
}

#[test]
fn roshan_waits_in_his_pit_and_the_pit_is_real() {
    let w = world();
    let roshan = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::Roshan)
        .map(|(id, u)| {
            assert_eq!(u.pos, rules::ROSHAN_PIT, "standing on the spawner point");
            assert_eq!(u.camp, rules::ROSHAN_PIT, "leashed to the pit");
            id
        });
    assert!(roshan.is_some(), "Roshan lives at tick zero");
    assert!(w.grid.walkable(rules::ROSHAN_PIT), "the pit floor is open");
}

#[test]
fn killing_roshan_pays_the_team_and_starts_the_grave_clock() {
    let mut w = world();
    let roshan = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::Roshan)
        .map(|(id, _)| id)
        .expect("Roshan lives");
    w.units.get_mut(roshan).unwrap().hp = 30;
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = rules::ROSHAN_PIT + Vec2::from_ints(250, 0);
    w.units.get_mut(hero).unwrap().max_hp = 50_000;
    w.units.get_mut(hero).unwrap().hp = 50_000;
    let gold_before = w.seat(SlotId(0)).unwrap().gold;
    let xp_before = w.seat(SlotId(0)).unwrap().xp;
    w.step(&[cmd(0, Order::AttackUnit { target: roshan })]);
    for _ in 0..300 {
        if !w.units.contains(roshan) {
            break;
        }
        w.step(&[]);
    }
    assert!(!w.units.contains(roshan), "Roshan falls");
    let seat = w.seat(SlotId(0)).unwrap();
    assert!(
        seat.gold >= gold_before + rules::ROSHAN_BOUNTY + rules::ROSHAN_TEAM_GOLD,
        "the bounty and the team gold arrived"
    );
    assert!(seat.xp > xp_before, "and the experience");
    let wait = w.roshan_respawn;
    assert!(
        (rules::ROSHAN_RESPAWN_MIN_TICKS
            ..=rules::ROSHAN_RESPAWN_MIN_TICKS + rules::ROSHAN_RESPAWN_SPREAD_TICKS)
            .contains(&wait),
        "the grave clock is running: {wait}"
    );
    // Fast-forward the grave: he returns to the pit.
    w.roshan_respawn = 3;
    for _ in 0..4 {
        w.step(&[]);
    }
    // The hero still standing in the pit gets his attention at once, so a
    // step may already be taken; back in the pit is what counts.
    assert!(
        w.units.iter().any(|(_, u)| u.kind == UnitKind::Roshan
            && u.pos.within(rules::ROSHAN_PIT, rules::units(300))),
        "Roshan returns"
    );
}

#[test]
fn a_neutral_kill_pays_the_killer_and_feeds_xp() {
    let mut w = world();
    let camp = rules::NEUTRAL_CAMPS[0];
    step_to_first_camps(&mut w);
    let neutral = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::CreepNeutral && u.camp == camp)
        .map(|(id, _)| id)
        .expect("the camp is stocked");
    w.units.get_mut(neutral).unwrap().hp = 20;
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = camp + Vec2::from_ints(250, 0);
    w.units.get_mut(hero).unwrap().hp = 5000;
    w.units.get_mut(hero).unwrap().max_hp = 5000;
    let gold_before = w.seat(SlotId(0)).unwrap().gold;
    let xp_before = w.seat(SlotId(0)).unwrap().xp;
    w.step(&[cmd(0, Order::AttackUnit { target: neutral })]);
    for _ in 0..300 {
        if !w.units.contains(neutral) {
            break;
        }
        w.step(&[]);
    }
    assert!(!w.units.contains(neutral), "the neutral dies");
    step_n(&mut w, 2);
    let seat = w.seat(SlotId(0)).unwrap();
    assert!(
        seat.gold >= gold_before + rules::NEUTRAL_BOUNTY,
        "the bounty arrived"
    );
    assert!(seat.xp > xp_before, "and the experience");
}
