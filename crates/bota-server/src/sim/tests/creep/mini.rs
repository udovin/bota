//! The small test map: that it is a map, and that creeps behave on it.

use bota_proto::{MapId, Team, UnitKind, Vec2};

use crate::sim::tests::fixtures::{mini_map, mini_world};
use crate::sim::{Terrain, rules};

#[test]
fn the_mini_map_is_open_ground_with_one_lane() {
    let map = mini_map();
    assert_eq!(map.id, MapId(1));
    assert_eq!(map.terrain, Terrain::Flat);
    assert!(!map.trees);
    assert_eq!(map.lanes, 1);
    assert_eq!(map.radiant_towers.len(), 3);
    assert_eq!(map.dire_towers.len(), 3);
    assert_eq!(map.camps.len(), 2, "one camp a side");
    let w = mini_world();
    assert!(
        crate::sim::tree_positions(w.map).is_empty(),
        "no forest to build"
    );
    // Every landmark stands on ground a unit may reach.
    for &(_, _, at) in map.radiant_towers.iter().chain(map.dire_towers) {
        assert!(w.ground.tier(at) > 0, "flat ground has one elevation");
    }
}

#[test]
fn the_mini_map_stands_its_buildings_and_fills_its_camps() {
    let mut w = mini_world();
    let towers = w
        .units
        .iter()
        .filter(|(_, u)| u.kind == UnitKind::Tower)
        .count();
    assert_eq!(towers, 6, "three a side");
    let ancients = w
        .units
        .iter()
        .filter(|(_, u)| u.kind == UnitKind::Ancient)
        .count();
    assert_eq!(ancients, 2);
    while w.tick < rules::FIRST_NEUTRAL_TICK {
        w.step(&[]);
    }
    let neutrals = w
        .units
        .iter()
        .filter(|(_, u)| u.kind == UnitKind::CreepNeutral)
        .count();
    assert!(neutrals > 0, "the jungle filled");
}

#[test]
fn a_wave_on_the_mini_map_marches_the_one_lane() {
    let mut w = mini_world();
    while w.tick < rules::FIRST_WAVE_TICK {
        w.step(&[]);
    }
    let creeps = w.units.iter().filter(|(_, u)| u.is_creep()).count();
    assert_eq!(creeps, 8, "four a side, one lane only");
    let route = &crate::sim::lane_routes(w.map)[crate::sim::team_index(Team::Radiant)]
        [usize::from(rules::LANE_MID)];
    assert!(
        route.last().unwrap().within(
            crate::sim::ancient_pos(w.map, Team::Dire),
            rules::units(200)
        ),
        "and its route ends at the enemy Ancient"
    );
    // The lane runs straight across, so the route holds one row.
    for point in route {
        assert!(
            (point.y - Vec2::from_ints(0, 9216).y).raw.abs() < rules::units(400).raw,
            "the lane is a straight run: {point:?}"
        );
    }
}

#[test]
fn the_two_maps_get_their_own_routes() {
    let mini = mini_world();
    let dota = crate::sim::tests::fixtures::world();
    let a = &crate::sim::lane_routes(mini.map)[0][0];
    let b = &crate::sim::lane_routes(dota.map)[0][0];
    let _ = (&mini.grid, &dota.grid);
    assert_ne!(a, b, "the route cache is keyed by map, not shared");
}
