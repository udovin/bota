//! World composition, hashing and fog projection.

use bota_proto::{SlotId, Team, UnitKind, Vec2};

use super::fixtures::{hero_id, world};
use crate::sim::Command;

#[test]
fn a_fresh_world_has_the_map_furniture_and_the_heroes() {
    let w = world();
    let count = |kind: UnitKind| w.units.iter().filter(|(_, u)| u.kind == kind).count();
    assert_eq!(count(UnitKind::Fountain), 2);
    assert_eq!(count(UnitKind::Ancient), 2);
    assert_eq!(count(UnitKind::Tower), 2);
    assert_eq!(count(UnitKind::Hero), 2);
    assert_eq!(w.tick, 0);
    assert_eq!(w.winner(), None);
    assert_eq!(w.seats.len(), 2);
    assert!(w.seats.iter().all(|s| s.unit.is_some()));
}

#[test]
fn two_worlds_from_the_same_config_hash_the_same() {
    assert_eq!(world().hash(), world().hash());
}

#[test]
fn a_step_moves_the_hash() {
    let mut w = world();
    let before = w.hash();
    w.step(&[]);
    assert_ne!(before, w.hash());
}

#[test]
fn hidden_state_moves_the_hash() {
    let mut a = world();
    let b = world();
    let hero = hero_id(&a, 0);
    // A cooldown is invisible in any view but part of the state.
    a.units.get_mut(hero).unwrap().attack_cooldown = 17;
    assert_ne!(a.hash(), b.hash());
}

#[test]
fn fog_hides_the_far_side_of_the_map() {
    let w = world();
    let radiant = w.view(Team::Radiant);
    // Nothing Radiant owns stands within vision of the Dire base.
    assert!(
        radiant
            .units
            .iter()
            .all(|u| u.team == Team::Radiant || u.pos.x.to_int() < 4096)
    );
    let dire_hero = hero_id(&w, 1);
    assert!(radiant.units.iter().all(|u| u.id != dire_hero));
}

#[test]
fn a_spectator_sees_everything() {
    let w = world();
    let full = w.view_full();
    assert_eq!(full.viewer, None);
    assert_eq!(full.units.len(), 8);
    assert!(full.players.iter().all(|p| p.gold.is_some()));
}

#[test]
fn enemy_gold_is_absent_from_a_team_view() {
    let w = world();
    let radiant = w.view(Team::Radiant);
    for p in &radiant.players {
        assert_eq!(
            p.gold.is_some(),
            p.team == Team::Radiant,
            "slot {:?}",
            p.slot
        );
    }
}

#[test]
fn own_units_are_always_in_view() {
    let w = world();
    let radiant = w.view(Team::Radiant);
    let own = radiant
        .units
        .iter()
        .filter(|u| u.team == Team::Radiant)
        .count();
    assert_eq!(own, 4, "fountain, ancient, tower, hero");
}

#[test]
fn views_are_sorted_by_entity_id() {
    let w = world();
    let full = w.view_full();
    let mut sorted = full.units.clone();
    sorted.sort_by_key(|u| u.id);
    assert_eq!(full.units, sorted);
}

#[test]
fn an_enemy_near_a_friendly_unit_is_visible() {
    let mut w = world();
    let dire_hero = hero_id(&w, 1);
    let radiant_hero = hero_id(&w, 0);
    let near = Vec2::from_ints(700, 700);
    w.units.get_mut(dire_hero).unwrap().pos = near;
    assert!(w.can_see(Team::Radiant, dire_hero));
    assert!(w.can_see(Team::Radiant, radiant_hero), "own unit");
    let view = w.view(Team::Radiant);
    assert!(view.units.iter().any(|u| u.id == dire_hero));
}

#[test]
fn a_finished_world_refuses_to_step() {
    let mut w = world();
    w.winner = Some(Team::Radiant);
    let tick = w.tick;
    let events = w.step(&[Command {
        slot: SlotId(0),
        order: bota_proto::Order::Stop,
    }]);
    assert!(events.is_empty());
    assert_eq!(w.tick, tick);
}
