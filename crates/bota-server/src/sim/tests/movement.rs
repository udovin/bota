//! Integer geometry behaves.

use bota_proto::{Fixed, Team, Vec2};

use crate::sim::{
    Arena, PassGrid, Unit, clamp_to_map, facing_towards, isqrt64, move_towards, per_tick, rules,
};

#[test]
fn isqrt_is_exact_on_squares_and_rounds_down_between() {
    assert_eq!(isqrt64(0), 0);
    assert_eq!(isqrt64(1), 1);
    assert_eq!(isqrt64(4), 2);
    assert_eq!(isqrt64(35), 5);
    assert_eq!(isqrt64(36), 6);
    assert_eq!(isqrt64(1 << 40), 1 << 20);
    assert_eq!(isqrt64((1 << 40) - 1), (1 << 20) - 1);
}

#[test]
fn per_tick_splits_a_second_into_ticks() {
    let speed = rules::units(300);
    assert_eq!(per_tick(speed), rules::units(10));
}

#[test]
fn move_towards_arrives_without_overshooting() {
    let from = Vec2::from_ints(100, 100);
    let to = Vec2::from_ints(100, 103);
    let step = rules::units(10);
    assert_eq!(move_towards(from, to, step), to);
}

#[test]
fn move_towards_covers_the_step_length() {
    let from = Vec2::from_ints(0, 0);
    let to = Vec2::from_ints(1000, 0);
    let after = move_towards(from, to, rules::units(10));
    assert_eq!(after, Vec2::from_ints(10, 0));
}

#[test]
fn move_towards_diagonal_keeps_the_speed() {
    let from = Vec2::from_ints(0, 0);
    let to = Vec2::from_ints(3000, 4000);
    let after = move_towards(from, to, rules::units(100));
    // A 3-4-5 triangle: one hundred units split 60/80.
    assert_eq!(after.x.to_int(), 60);
    assert_eq!(after.y.to_int(), 80);
}

#[test]
fn facing_matches_the_axes_and_diagonals() {
    let o = Vec2::from_ints(100, 100);
    let cases = [
        (Vec2::from_ints(200, 100), 0u16),
        (Vec2::from_ints(200, 200), 8192),
        (Vec2::from_ints(100, 200), 16384),
        (Vec2::from_ints(0, 100), 32768),
        (Vec2::from_ints(100, 0), 49152),
    ];
    for (target, brads) in cases {
        assert_eq!(facing_towards(o, target).brads, brads, "{target:?}");
    }
}

#[test]
fn clamp_keeps_positions_on_the_map() {
    let inside = Vec2::from_ints(100, 16000);
    assert_eq!(clamp_to_map(inside), inside);
    let outside = Vec2 {
        x: Fixed::from_int(-5),
        y: Fixed::from_int(rules::MAP_SIZE + 600),
    };
    let clamped = clamp_to_map(outside);
    assert_eq!(clamped.x, Fixed::ZERO);
    assert_eq!(clamped.y, Fixed::from_int(rules::MAP_SIZE));
}

#[test]
fn the_open_grid_covers_the_map_and_nothing_else() {
    let grid = PassGrid::open();
    assert!(grid.walkable(Vec2::from_ints(0, 0)));
    assert!(grid.walkable(Vec2::from_ints(4000, 4000)));
    assert!(!grid.walkable(Vec2 {
        x: Fixed::from_int(-1),
        y: Fixed::ZERO,
    }));
    assert!(!grid.walkable(Vec2::from_ints(rules::MAP_SIZE, 0)));
}

#[test]
fn turning_is_clamped_to_the_rate() {
    use bota_proto::Angle;
    let east = Angle { brads: 0 };
    let west = Angle { brads: 32768 };
    let one_tick = crate::sim::turn_towards(east, west, rules::TURN_RATE_BRADS);
    assert_eq!(one_tick.brads, rules::TURN_RATE_BRADS);
    let arrived = crate::sim::turn_towards(Angle { brads: 30000 }, west, rules::TURN_RATE_BRADS);
    assert_eq!(arrived.brads, 32768, "a short turn finishes in one tick");
}

#[test]
fn turning_wraps_the_short_way_around() {
    use bota_proto::Angle;
    let from = Angle { brads: 1000 };
    let to = Angle { brads: 64000 };
    let turned = crate::sim::turn_towards(from, to, rules::TURN_RATE_BRADS);
    assert_eq!(
        turned.brads, 64000,
        "2536 brads backwards, not 63000 forwards"
    );
    assert_eq!(crate::sim::facing_gap(from, to), 2536);
}

#[test]
fn lane_offset_grows_away_from_the_centerline() {
    // A tower stands on its own lane's centerline by construction.
    assert_eq!(
        crate::sim::lane_offset_squared(rules::LANE_MID, Vec2::from_ints(6026, 6290)),
        0
    );
    let off = crate::sim::lane_offset_squared(rules::LANE_MID, Vec2::from_ints(6100, 9200));
    let leash = i64::from(rules::units(rules::LANE_LEASH).raw);
    assert!(off > leash * leash, "deep jungle is past the leash");
    // A point on the west column is right at home for the top lane.
    let top = crate::sim::lane_offset_squared(rules::LANE_TOP, Vec2::from_ints(2715, 8344));
    assert!(top < rules::units(100).squared_raw());
}

#[test]
fn a_step_into_another_unit_is_refused_and_a_step_out_is_not() {
    let mut units = Arena::new();
    units.insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(1000, 1000)));
    let walker = units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(1020, 1000),
    ));
    let from = Vec2::from_ints(1020, 1000);
    assert!(crate::sim::blocked_by_units(
        &units,
        walker,
        from,
        Vec2::from_ints(1013, 1000)
    ));
    assert!(!crate::sim::blocked_by_units(
        &units,
        walker,
        from,
        Vec2::from_ints(1031, 1000)
    ));
}

#[test]
fn steering_aims_tangent_around_a_standing_body() {
    let mut units = Arena::new();
    let stander = units.insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(1000, 1000)));
    let walker = units.insert(Unit::melee_creep(Team::Radiant, Vec2::from_ints(500, 1000)));
    let waypoint = Vec2::from_ints(1500, 1000);
    let aim = crate::sim::steer_target(&units, walker, Vec2::from_ints(500, 1000), waypoint);
    assert_ne!(aim, waypoint, "the direct line is refused");
    let inflated = units.get(walker).unwrap().radius
        + units.get(stander).unwrap().radius
        + rules::units(rules::STEER_MARGIN);
    let clear = i64::from(inflated.raw) - i64::from(Fixed::EPSILON.raw) * 4;
    assert!(
        crate::sim::segment_distance_squared(
            Vec2::from_ints(1000, 1000),
            Vec2::from_ints(500, 1000),
            aim
        ) >= clear * clear,
        "the leg to the tangent grazes the circle, never cuts it: {aim:?}"
    );
}

#[test]
fn steering_ignores_walking_bodies_and_the_goal_sitter() {
    let mut units = Arena::new();
    let mover = units.insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(1000, 1000)));
    units.get_mut(mover).unwrap().moving = true;
    let walker = units.insert(Unit::melee_creep(Team::Radiant, Vec2::from_ints(500, 1000)));
    let waypoint = Vec2::from_ints(1500, 1000);
    assert_eq!(
        crate::sim::steer_target(&units, walker, Vec2::from_ints(500, 1000), waypoint),
        waypoint,
        "a walking body is not steered around"
    );
    units.get_mut(mover).unwrap().moving = false;
    units.get_mut(mover).unwrap().pos = Vec2::from_ints(1500, 1000);
    assert_eq!(
        crate::sim::steer_target(&units, walker, Vec2::from_ints(500, 1000), waypoint),
        waypoint,
        "neither is whoever occupies the goal itself"
    );
}

#[test]
fn a_unit_slide_finds_a_free_sidestep() {
    let mut units = Arena::new();
    units.insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(1000, 1000)));
    let walker = units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(1020, 1000),
    ));
    let from = Vec2::from_ints(1020, 1000);
    let slide = crate::sim::unit_slide(
        &units,
        &PassGrid::open(),
        walker,
        Vec2::from_ints(900, 1000),
        rules::units(11),
    )
    .expect("the way around is open");
    assert_ne!(slide, from, "it does go somewhere");
    assert!(
        !crate::sim::blocked_by_units(&units, walker, from, slide),
        "and the sidestep itself is legal"
    );
}

#[test]
fn a_path_goes_around_blocked_cells() {
    let mut grid = PassGrid::open();
    grid.block_circle(Vec2::from_ints(1000, 1000), rules::units(150));
    let from = Vec2::from_ints(500, 1000);
    let to = Vec2::from_ints(1500, 1000);
    assert!(
        !crate::sim::grid_los(&grid, from, to),
        "the straight line is blocked"
    );
    assert!(crate::sim::grid_los(
        &grid,
        from,
        Vec2::from_ints(500, 1500)
    ));
    let path = crate::sim::find_path(&grid, from, to);
    assert!(!path.is_empty(), "a way around exists");
    for w in &path {
        assert!(grid.walkable(*w), "waypoint {w:?} is walkable");
    }
    let last = path.last().unwrap();
    assert!(last.within(to, rules::units(rules::GRID_CELL_SIZE)));
}

#[test]
fn a_fallen_structure_reopens_its_ground() {
    let mut grid = PassGrid::open();
    let center = Vec2::from_ints(2304, 2304);
    let clearance = crate::sim::structure_clearance(rules::units(rules::TOWER_RADIUS));
    grid.block_circle(center, clearance);
    assert!(!grid.walkable(center));
    grid.open_circle(center, clearance);
    assert!(grid.walkable(center));
}
