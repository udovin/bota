//! Target priority: classes, the tie band, and what a hero is doing.

use bota_proto::{Order, SlotId, Team, UnitKind, Vec2};

use crate::sim::tests::fixtures::{hero_id, world};
use crate::sim::{Command, PriorityOrder, Unit, acquire, hostile, rules};

fn cmd(slot: u8, order: Order) -> Command {
    Command {
        slot: SlotId(slot),
        order,
    }
}

#[test]
fn a_creep_outranks_a_building_however_close_the_building_is() {
    let mut w = world();
    let seeker = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(7772, 7908)));
    let far = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8100, 8200),
    ));
    let pick = acquire(
        &w,
        seeker,
        rules::units(rules::MELEE_CREEP_ACQUISITION),
        PriorityOrder::Normal,
    );
    assert_eq!(
        pick,
        Some(far),
        "the tier one tower stands closer and loses"
    );
}

#[test]
fn a_siege_creep_drops_everything_for_a_building() {
    let mut w = world();
    let seeker = w
        .units
        .insert(Unit::siege_creep(Team::Dire, Vec2::from_ints(8100, 8200)));
    let creep = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8140, 8240),
    ));
    let pick = acquire(
        &w,
        seeker,
        rules::units(rules::SIEGE_CREEP_ACQUISITION),
        PriorityOrder::SiegeFirst,
    );
    assert_ne!(pick, Some(creep), "the adjacent creep loses to the tower");
    assert_eq!(
        pick.and_then(|p| w.units.get(p)).map(|u| u.kind),
        Some(UnitKind::Tower)
    );
}

#[test]
fn an_equally_close_hero_attacking_our_heroes_is_taken_first() {
    let mut w = world();
    let radiant_hero = hero_id(&w, 0);
    let dire_hero = hero_id(&w, 1);
    w.units.get_mut(radiant_hero).unwrap().pos = Vec2::from_ints(8196, 8196);
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(8496, 8496);
    let seeker = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8396, 8196)));
    let bystander = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8296, 8096),
    ));
    // Nobody has orders yet: the closer creep wins on distance alone.
    let pick = acquire(
        &w,
        seeker,
        rules::units(rules::MELEE_CREEP_ACQUISITION),
        PriorityOrder::Normal,
    );
    assert_eq!(pick, Some(bystander));
    // Once the hero attacks one of ours it wins the tie band instead.
    w.step(&[cmd(0, Order::AttackUnit { target: dire_hero })]);
    let pick = acquire(
        &w,
        seeker,
        rules::units(rules::MELEE_CREEP_ACQUISITION),
        PriorityOrder::Normal,
    );
    assert_eq!(pick, Some(radiant_hero));
}

#[test]
fn distance_beats_behaviour_outside_the_tie_band() {
    let mut w = world();
    let radiant_hero = hero_id(&w, 0);
    let dire_hero = hero_id(&w, 1);
    w.units.get_mut(radiant_hero).unwrap().pos = Vec2::from_ints(8600, 8196);
    w.units.get_mut(dire_hero).unwrap().pos = Vec2::from_ints(8900, 8496);
    let seeker = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8396, 8196)));
    let close = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8436, 8196),
    ));
    w.step(&[cmd(0, Order::AttackUnit { target: dire_hero })]);
    let pick = acquire(
        &w,
        seeker,
        rules::units(rules::MELEE_CREEP_ACQUISITION),
        PriorityOrder::Normal,
    );
    assert_eq!(pick, Some(close), "204 apart is not about equally close");
}

#[test]
fn lane_creeps_fight_only_the_pull_camps() {
    let w = world();
    let pull = w.map.camps.iter().find(|c| c.pullable).unwrap();
    let quiet = w.map.camps.iter().find(|c| !c.pullable).unwrap();
    let creep = Unit::melee_creep(Team::Radiant, Vec2::ZERO);
    let on_pull = Unit::neutral_creep(crate::sim::NeutralKind::Kobold, pull.pos, 0, 0);
    let on_quiet = Unit::neutral_creep(crate::sim::NeutralKind::Kobold, quiet.pos, 1, 0);
    assert!(hostile(&creep, &on_pull), "the pull camp is fair game");
    assert!(!hostile(&creep, &on_quiet), "every other camp is ignored");
    assert!(
        hostile(&on_quiet, &creep),
        "the jungle hits back regardless"
    );
    let tower = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::Tower)
        .map(|(_, u)| u.clone())
        .unwrap();
    assert!(!hostile(&tower, &on_pull), "towers never shoot the jungle");
}
