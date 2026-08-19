//! Neutral camps: what wakes them, how far they go, and how they let go.

use bota_proto::{Team, UnitKind, Vec2};

use crate::sim::tests::fixtures::{hero_id, world};
use crate::sim::{CreepAi, Unit, World, rules};

fn step_n(w: &mut World, n: u32) {
    for _ in 0..n {
        w.step(&[]);
    }
}

fn neutral_ai(w: &World, id: bota_proto::EntityId) -> crate::sim::NeutralAi {
    match w.units.get(id).unwrap().ai.clone() {
        Some(CreepAi::Neutral(ai)) => ai,
        _ => panic!("a neutral carries a neutral ai"),
    }
}

/// A lone kobold on open ground, with a hero placed `away` from it.
fn camped(away: i32) -> (World, bota_proto::EntityId, bota_proto::EntityId) {
    let mut w = world();
    let home = Vec2::from_ints(8300, 8300);
    let neutral = w.units.insert(Unit::neutral_creep(
        crate::sim::NeutralKind::Kobold,
        home,
        0,
        0,
    ));
    w.units.get_mut(neutral).unwrap().move_speed = bota_proto::Fixed::ZERO;
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = home + Vec2::from_ints(away, 0);
    w.units.get_mut(hero).unwrap().move_speed = bota_proto::Fixed::ZERO;
    w.units.get_mut(hero).unwrap().hp = 1_000_000;
    w.units.get_mut(hero).unwrap().max_hp = 1_000_000;
    (w, neutral, hero)
}

#[test]
fn a_neutral_sleeps_until_something_comes_inside_the_aggro_radius() {
    // Just outside: nothing happens, however long you stand there.
    let (mut w, neutral, _) = camped(rules::NEUTRAL_AGGRO_RANGE + 60);
    step_n(&mut w, 120);
    assert_eq!(
        w.units.get(neutral).unwrap().engage,
        None,
        "standing outside the aggro radius wakes nobody"
    );
    // Just inside: it wakes.
    let (mut w, neutral, hero) = camped(rules::NEUTRAL_AGGRO_RANGE - 60);
    w.step(&[]);
    assert_eq!(
        w.units.get(neutral).unwrap().engage,
        Some(hero),
        "inside the aggro radius it wakes"
    );
}

#[test]
fn the_aggro_radius_is_much_shorter_than_the_acquisition_range() {
    // The two are different numbers and the camp uses the short one to wake.
    let def = crate::sim::NeutralKind::Kobold.def();
    assert!(
        rules::NEUTRAL_AGGRO_RANGE < def.acquisition,
        "waking at {} is stricter than looking at {}",
        rules::NEUTRAL_AGGRO_RANGE,
        def.acquisition
    );
    let (mut w, neutral, _) = camped(def.acquisition - 60);
    step_n(&mut w, 60);
    assert_eq!(
        w.units.get(neutral).unwrap().engage,
        None,
        "inside acquisition but outside the aggro radius is still asleep"
    );
}

#[test]
fn the_window_only_runs_beyond_the_guard_distance() {
    let (mut w, neutral, hero) = camped(rules::NEUTRAL_AGGRO_RANGE - 60);
    w.step(&[]);
    assert_eq!(w.units.get(neutral).unwrap().engage, Some(hero));
    // Standing still inside the guard distance, the window never spends.
    step_n(&mut w, rules::NEUTRAL_AGGRO_WINDOW * 3);
    assert_eq!(
        w.units.get(neutral).unwrap().engage,
        Some(hero),
        "no timer runs while it is home"
    );
    assert_eq!(
        neutral_ai(&w, neutral).leash_left,
        rules::NEUTRAL_AGGRO_WINDOW
    );
}

#[test]
fn a_neutral_dragged_out_lets_go_when_its_window_runs_out() {
    let (mut w, neutral, hero) = camped(rules::NEUTRAL_AGGRO_RANGE - 60);
    w.step(&[]);
    assert_eq!(w.units.get(neutral).unwrap().engage, Some(hero));
    // Teleport it past the guard distance without letting it walk.
    let home = neutral_ai(&w, neutral).home;
    w.units.get_mut(neutral).unwrap().pos =
        home + Vec2::from_ints(rules::NEUTRAL_GUARD_DISTANCE + 200, 0);
    w.units.get_mut(hero).unwrap().pos =
        home + Vec2::from_ints(rules::NEUTRAL_GUARD_DISTANCE + 300, 0);
    step_n(&mut w, rules::NEUTRAL_AGGRO_WINDOW - 2);
    assert_eq!(
        w.units.get(neutral).unwrap().engage,
        Some(hero),
        "still awake inside the window"
    );
    step_n(&mut w, 3);
    let ai = neutral_ai(&w, neutral);
    assert_eq!(
        w.units.get(neutral).unwrap().engage,
        None,
        "the window ran out"
    );
    assert!(ai.going_home, "and it heads back");
    assert_eq!(
        ai.reaggro_block,
        rules::NEUTRAL_REAGGRO_BLOCK,
        "damage cannot wake it for a moment"
    );
    assert_eq!(
        ai.next_window,
        rules::NEUTRAL_SHORT_WINDOW,
        "and the next window is the short one"
    );
}

#[test]
fn standing_close_cannot_wake_a_neutral_walking_home() {
    let (mut w, neutral, hero) = camped(rules::NEUTRAL_AGGRO_RANGE - 60);
    w.step(&[]);
    let home = neutral_ai(&w, neutral).home;
    w.units.get_mut(neutral).unwrap().pos =
        home + Vec2::from_ints(rules::NEUTRAL_GUARD_DISTANCE + 200, 0);
    step_n(&mut w, rules::NEUTRAL_AGGRO_WINDOW + 2);
    assert!(neutral_ai(&w, neutral).going_home);
    // The hero stands right on top of it and does nothing.
    let at = w.units.get(neutral).unwrap().pos;
    w.units.get_mut(hero).unwrap().pos = at + Vec2::from_ints(40, 0);
    step_n(&mut w, 60);
    assert_eq!(
        w.units.get(neutral).unwrap().engage,
        None,
        "proximity is deaf until it is home"
    );
}

#[test]
fn a_camp_carries_its_kind_and_its_upgrades() {
    let khan = crate::sim::NeutralKind::CentaurKhan;
    let def = khan.def();
    assert_eq!(def.hp, 1100, "straight out of npc_units.txt");
    assert!(!def.ancient);
    let up = crate::sim::upgraded(def, 4);
    assert_eq!(up.hp, 1100 + 4 * rules::NEUTRAL_UPGRADE_HP);
    assert_eq!(up.damage, def.damage + 4 * rules::NEUTRAL_UPGRADE_DAMAGE);
    assert_eq!(up.armor, def.armor + 2, "half a point an interval");
    let mut w = world();
    let unit = w
        .units
        .insert(Unit::neutral_creep(khan, Vec2::from_ints(8300, 8300), 3, 4));
    let u = w.units.get(unit).unwrap();
    assert_eq!(u.max_hp, up.hp);
    assert_eq!(u.team, Team::Neutral);
    assert_eq!(u.kind, UnitKind::CreepNeutral);
}

#[test]
fn every_camp_holds_a_roster_of_its_own_size() {
    for def in crate::sim::tests::fixtures::dota_map().camps {
        let count = crate::sim::rosters_of(def.kind).count();
        assert!(count >= 2, "{:?} needs choices to avoid repeats", def.kind);
    }
    // The published roster counts per category.
    let of = |k| crate::sim::rosters_of(k).count();
    assert_eq!(of(crate::sim::CampKind::Small), 6);
    assert_eq!(of(crate::sim::CampKind::Medium), 5);
    assert_eq!(of(crate::sim::CampKind::Large), 6);
    assert_eq!(of(crate::sim::CampKind::Ancient), 4);
}
