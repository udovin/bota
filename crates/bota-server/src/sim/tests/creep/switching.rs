//! Holding a target, and the four ways of losing it.

use bota_proto::{Fixed, Order, SlotId, Team, UnitKind, Vec2};

use crate::sim::tests::fixtures::{aim_along_lane, hero_id, world};
use crate::sim::{Command, Unit, World, rules};

fn cmd(slot: u8, order: Order) -> Command {
    Command {
        slot: SlotId(slot),
        order,
    }
}

fn step_n(w: &mut World, n: u32) {
    for _ in 0..n {
        w.step(&[]);
    }
}

#[test]
fn a_creep_on_a_building_switches_the_moment_a_creep_arrives() {
    let mut w = world();
    let (tower, at) = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::Tower && u.team == Team::Radiant && u.tier == 1)
        .map(|(id, u)| (id, u.pos))
        .unwrap();
    let creep = w.units.insert(Unit::melee_creep(
        Team::Dire,
        Vec2::from_ints(at.x.to_int() + 120, at.y.to_int()),
    ));
    w.units.get_mut(creep).unwrap().move_speed = Fixed::ZERO;
    w.step(&[]);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(tower),
        "nothing else is in range, so the building it is"
    );
    let rival = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(at.x.to_int() + 200, at.y.to_int()),
    ));
    w.units.get_mut(rival).unwrap().move_speed = Fixed::ZERO;
    w.step(&[]);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(rival),
        "a unit outranks a building the tick it comes into range"
    );
}

#[test]
fn a_closer_arrival_steals_nothing() {
    let mut w = world();
    let busy_with = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8380, 8300),
    ));
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8300, 8300)));
    w.units.get_mut(creep).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(busy_with).unwrap().move_speed = Fixed::ZERO;
    w.step(&[]);
    assert_eq!(w.units.get(creep).unwrap().engage, Some(busy_with));
    // Something of the same class walking up closer changes nothing while
    // the creep can still hit what it holds.
    let near = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8340, 8300),
    ));
    w.units.get_mut(near).unwrap().move_speed = Fixed::ZERO;
    step_n(&mut w, 30);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(busy_with),
        "a busy creep keeps what it is on"
    );
}

#[test]
fn a_hero_walking_up_steals_nothing() {
    let mut w = world();
    let busy_with = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8380, 8300),
    ));
    w.units.get_mut(busy_with).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(busy_with).unwrap().hp = 1_000_000;
    w.units.get_mut(busy_with).unwrap().max_hp = 1_000_000;
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8300, 8300)));
    w.units.get_mut(creep).unwrap().move_speed = Fixed::ZERO;
    w.step(&[]);
    assert_eq!(w.units.get(creep).unwrap().engage, Some(busy_with));
    // The hero steps right up to it and issues no order at all.
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8340, 8300);
    w.units.get_mut(hero).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(hero).unwrap().hp = 1_000_000;
    step_n(&mut w, 200);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(busy_with),
        "walking close is not an aggro check"
    );
}

#[test]
fn an_attack_order_takes_the_wave_over_a_closer_creep() {
    let mut w = world();
    let radiant_hero = hero_id(&w, 0);
    let dire_hero = hero_id(&w, 1);
    // The hero stands well behind its own creep, as it would in a lane.
    w.units.get_mut(radiant_hero).unwrap().pos = Vec2::from_ints(8000, 8300);
    w.units.get_mut(radiant_hero).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(8600, 8600);
    let shield = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8260, 8300),
    ));
    w.units.get_mut(shield).unwrap().move_speed = Fixed::ZERO;
    let guard = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8300, 8300)));
    aim_along_lane(&mut w, guard, rules::LANE_MID);
    w.units.get_mut(guard).unwrap().move_speed = Fixed::ZERO;
    w.step(&[]);
    assert_eq!(
        w.units.get(guard).unwrap().engage,
        Some(shield),
        "on its own it takes the creep in its face"
    );
    // Three hundred units behind its own creep, the click still lands.
    w.step(&[cmd(0, Order::AttackUnit { target: dire_hero })]);
    assert_eq!(
        w.units.get(guard).unwrap().engage,
        Some(radiant_hero),
        "the order takes the wave outright, closer bystanders or not"
    );
}

#[test]
fn the_pull_lets_go_once_the_hold_runs_out() {
    let mut w = world();
    let radiant_hero = hero_id(&w, 0);
    let dire_hero = hero_id(&w, 1);
    w.units.get_mut(radiant_hero).unwrap().pos = Vec2::from_ints(8000, 8300);
    w.units.get_mut(radiant_hero).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(radiant_hero).unwrap().hp = 1_000_000;
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(8600, 8600);
    let shield = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8260, 8300),
    ));
    w.units.get_mut(shield).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(shield).unwrap().hp = 1_000_000;
    w.units.get_mut(shield).unwrap().max_hp = 1_000_000;
    let guard = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8300, 8300)));
    aim_along_lane(&mut w, guard, rules::LANE_MID);
    w.units.get_mut(guard).unwrap().move_speed = Fixed::ZERO;
    w.step(&[cmd(0, Order::AttackUnit { target: dire_hero })]);
    assert_eq!(w.units.get(guard).unwrap().engage, Some(radiant_hero));
    step_n(&mut w, rules::ORDER_AGGRO_HOLD_TICKS + 2);
    assert_eq!(
        w.units.get(guard).unwrap().engage,
        Some(shield),
        "the hold expires and the ranking takes it back"
    );
}

#[test]
fn a_target_in_range_is_held_indefinitely() {
    // The chase window is spent only by the target leaving acquisition range.
    // A creep trading blows in its lane must never let go on a timer.
    let mut w = world();
    let foe = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8700, 8300),
    ));
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8300, 8300)));
    for id in [foe, creep] {
        w.units.get_mut(id).unwrap().move_speed = Fixed::ZERO;
        w.units.get_mut(id).unwrap().hp = 1_000_000;
        w.units.get_mut(id).unwrap().max_hp = 1_000_000;
    }
    w.step(&[]);
    assert_eq!(w.units.get(creep).unwrap().engage, Some(foe));
    step_n(&mut w, rules::CREEP_CHASE_TICKS * 5);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(foe),
        "no timer takes a creep off a target it can reach"
    );
}

#[test]
fn a_pulled_creep_goes_back_to_the_wave_and_stays_there() {
    let mut w = world();
    let radiant_hero = hero_id(&w, 0);
    let dire_hero = hero_id(&w, 1);
    w.units.get_mut(radiant_hero).unwrap().pos = Vec2::from_ints(8000, 8300);
    w.units.get_mut(radiant_hero).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(radiant_hero).unwrap().hp = 1_000_000;
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(8600, 8600);
    let shield = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8260, 8300),
    ));
    w.units.get_mut(shield).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(shield).unwrap().hp = 1_000_000;
    w.units.get_mut(shield).unwrap().max_hp = 1_000_000;
    let guard = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8300, 8300)));
    aim_along_lane(&mut w, guard, rules::LANE_MID);
    w.units.get_mut(guard).unwrap().move_speed = Fixed::ZERO;
    w.step(&[cmd(0, Order::AttackUnit { target: dire_hero })]);
    assert_eq!(w.units.get(guard).unwrap().engage, Some(radiant_hero));
    step_n(&mut w, rules::ORDER_AGGRO_HOLD_TICKS + 2);
    assert_eq!(w.units.get(guard).unwrap().engage, Some(shield));
    // And it stays on the wave: the hero standing there is not a new check.
    step_n(&mut w, rules::CREEP_CHASE_TICKS * 4);
    assert_eq!(
        w.units.get(guard).unwrap().engage,
        Some(shield),
        "one pull, one return, and no drifting back"
    );
}

#[test]
fn a_hero_out_of_reach_loses_the_creep_to_a_nearer_one() {
    // Nobody clicked anything: the creep found the hero first. Once the hero
    // is no longer in reach, a creep it can actually hit takes over.
    let mut w = world();
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8600, 8300);
    w.units.get_mut(hero).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(hero).unwrap().hp = 1_000_000;
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8300, 8300)));
    aim_along_lane(&mut w, creep, rules::LANE_MID);
    w.units.get_mut(creep).unwrap().move_speed = Fixed::ZERO;
    w.step(&[]);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(hero),
        "the only enemy in range is the hero"
    );
    let shield = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8360, 8300),
    ));
    w.units.get_mut(shield).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(shield).unwrap().hp = 1_000_000;
    w.units.get_mut(shield).unwrap().max_hp = 1_000_000;
    w.step(&[]);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(shield),
        "no pull holds it and the hero is out of reach, so the creep wins"
    );
    step_n(&mut w, rules::CREEP_CHASE_TICKS * 3);
    assert_eq!(w.units.get(creep).unwrap().engage, Some(shield));
}

#[test]
fn a_ranged_creep_keeps_a_hero_it_can_reach_over_a_nearer_creep() {
    // The case that settles the rule: a ranged creep shooting a hero does not
    // drop it for a creep standing closer, because it is still hitting the
    // hero. Step out of its five hundred and it takes the creep instead.
    let mut w = world();
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8700, 8300);
    w.units.get_mut(hero).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(hero).unwrap().hp = 1_000_000;
    let shooter = w
        .units
        .insert(Unit::ranged_creep(Team::Dire, Vec2::from_ints(8300, 8300)));
    aim_along_lane(&mut w, shooter, rules::LANE_MID);
    w.units.get_mut(shooter).unwrap().move_speed = Fixed::ZERO;
    w.step(&[]);
    assert_eq!(w.units.get(shooter).unwrap().engage, Some(hero));
    let nearer = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8400, 8300),
    ));
    w.units.get_mut(nearer).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(nearer).unwrap().hp = 1_000_000;
    w.units.get_mut(nearer).unwrap().max_hp = 1_000_000;
    step_n(&mut w, 60);
    assert_eq!(
        w.units.get(shooter).unwrap().engage,
        Some(hero),
        "in reach means kept, however close the creep stands"
    );
    // The hero backs out of the shooter's range but stays in acquisition.
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8860, 8300);
    w.step(&[]);
    assert_eq!(
        w.units.get(shooter).unwrap().engage,
        Some(nearer),
        "out of reach, and there is something it can hit"
    );
}

#[test]
fn a_click_still_beats_a_creep_standing_in_the_way() {
    // The click is the only thing that makes an out-of-reach hero stick.
    let mut w = world();
    let radiant_hero = hero_id(&w, 0);
    let dire_hero = hero_id(&w, 1);
    w.units.get_mut(radiant_hero).unwrap().pos = Vec2::from_ints(8600, 8300);
    w.units.get_mut(radiant_hero).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(radiant_hero).unwrap().hp = 1_000_000;
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(8600, 8600);
    let shield = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8360, 8300),
    ));
    w.units.get_mut(shield).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(shield).unwrap().hp = 1_000_000;
    w.units.get_mut(shield).unwrap().max_hp = 1_000_000;
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8300, 8300)));
    aim_along_lane(&mut w, creep, rules::LANE_MID);
    w.units.get_mut(creep).unwrap().move_speed = Fixed::ZERO;
    w.step(&[]);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(shield),
        "left alone it takes the creep it can reach"
    );
    w.step(&[cmd(0, Order::AttackUnit { target: dire_hero })]);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(radiant_hero),
        "the click takes it off the creep"
    );
    step_n(&mut w, 30);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(radiant_hero),
        "and holds it there"
    );
    step_n(&mut w, rules::ORDER_AGGRO_HOLD_TICKS);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(shield),
        "until the hold runs out on a hero it cannot reach"
    );
}

#[test]
fn an_ally_click_sheds_the_creep_even_from_point_blank() {
    // Measured in game: the wave lets go of you and takes your creeps even
    // when you are by far the nearest thing to it.
    let mut w = world();
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8360, 8300);
    w.units.get_mut(hero).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(hero).unwrap().hp = 1_000_000;
    let shooter = w
        .units
        .insert(Unit::ranged_creep(Team::Dire, Vec2::from_ints(8300, 8300)));
    aim_along_lane(&mut w, shooter, rules::LANE_MID);
    w.units.get_mut(shooter).unwrap().move_speed = Fixed::ZERO;
    // The hero's own creep stands five times further away.
    let ally = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8600, 8300),
    ));
    w.units.get_mut(ally).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(ally).unwrap().hp = 1_000_000;
    w.units.get_mut(ally).unwrap().max_hp = 1_000_000;
    w.step(&[]);
    assert_eq!(
        w.units.get(shooter).unwrap().engage,
        Some(hero),
        "the hero is the nearest thing, so it is taken"
    );
    w.step(&[cmd(0, Order::AttackUnit { target: ally })]);
    assert_eq!(
        w.units.get(shooter).unwrap().engage,
        Some(ally),
        "the click lets the hero go however close it stands"
    );
    // A creep target is held, so the shed sticks.
    step_n(&mut w, rules::CREEP_CHASE_TICKS * 3);
    assert_eq!(w.units.get(shooter).unwrap().engage, Some(ally));
}

#[test]
fn an_ally_click_with_nothing_else_around_buys_nothing() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8360, 8300);
    w.units.get_mut(hero).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(hero).unwrap().hp = 1_000_000;
    let shooter = w
        .units
        .insert(Unit::ranged_creep(Team::Dire, Vec2::from_ints(8300, 8300)));
    aim_along_lane(&mut w, shooter, rules::LANE_MID);
    w.units.get_mut(shooter).unwrap().move_speed = Fixed::ZERO;
    // The ally is far out of the shooter's acquisition range.
    let ally = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(11000, 8300),
    ));
    w.units.get_mut(ally).unwrap().move_speed = Fixed::ZERO;
    w.step(&[]);
    assert_eq!(w.units.get(shooter).unwrap().engage, Some(hero));
    w.step(&[cmd(0, Order::AttackUnit { target: ally })]);
    assert_eq!(
        w.units.get(shooter).unwrap().engage,
        Some(hero),
        "with nobody else to take, the demoted hero is taken again at once"
    );
}

#[test]
fn a_creep_out_of_reach_loses_the_creep_the_same_way_a_hero_does() {
    // The rule is one rule: whatever the creep holds, losing reach of it is
    // what makes it look again. Nothing here is a hero.
    let mut w = world();
    let far = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8700, 8300),
    ));
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8300, 8300)));
    for id in [far, creep] {
        w.units.get_mut(id).unwrap().move_speed = Fixed::ZERO;
        w.units.get_mut(id).unwrap().hp = 1_000_000;
        w.units.get_mut(id).unwrap().max_hp = 1_000_000;
    }
    w.step(&[]);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(far),
        "acquired at four hundred, well outside melee reach"
    );
    let near = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8340, 8300),
    ));
    w.units.get_mut(near).unwrap().move_speed = Fixed::ZERO;
    w.step(&[]);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(near),
        "it cannot reach the far one, so the near one takes it"
    );
}

#[test]
fn a_building_is_left_only_for_what_the_creep_can_reach() {
    // Class preemption is measured in attack range, not acquisition range: a
    // unit the creep cannot hit yet does not take it off the tower.
    let mut w = world();
    let (tower, at) = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::Tower && u.team == Team::Radiant && u.tier == 1)
        .map(|(id, u)| (id, u.pos))
        .unwrap();
    let shooter = w.units.insert(Unit::ranged_creep(
        Team::Dire,
        Vec2::from_ints(at.x.to_int() + 400, at.y.to_int()),
    ));
    w.units.get_mut(shooter).unwrap().move_speed = Fixed::ZERO;
    w.step(&[]);
    assert_eq!(
        w.units.get(shooter).unwrap().engage,
        Some(tower),
        "nothing else is there, so the tower it is"
    );
    // Inside the shooter's acquisition (600) but outside its reach (500).
    let outside = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(at.x.to_int() + 400, at.y.to_int() + 560),
    ));
    w.units.get_mut(outside).unwrap().move_speed = Fixed::ZERO;
    w.step(&[]);
    assert_eq!(
        w.units.get(shooter).unwrap().engage,
        Some(tower),
        "a unit it cannot reach does not take it off the building"
    );
    // Inside the reach.
    let inside = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(at.x.to_int() + 400, at.y.to_int() + 300),
    ));
    w.units.get_mut(inside).unwrap().move_speed = Fixed::ZERO;
    w.step(&[]);
    assert_eq!(
        w.units.get(shooter).unwrap().engage,
        Some(inside),
        "a unit within reach takes it at once"
    );
}
