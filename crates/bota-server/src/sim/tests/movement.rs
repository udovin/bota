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
    let inside = Vec2::from_ints(100, 8000);
    assert_eq!(clamp_to_map(inside), inside);
    let outside = Vec2 {
        x: Fixed::from_int(-5),
        y: Fixed::from_int(9000),
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
fn overlapping_movers_are_pushed_apart() {
    let mut units = Arena::new();
    let a = units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(1000, 1000),
    ));
    let b = units.insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(1005, 1000)));
    crate::sim::separate_collisions(&mut units);
    let (pa, pb) = (units.get(a).unwrap().pos, units.get(b).unwrap().pos);
    // Both moved, symmetrically along the axis between them.
    assert!(pa.x < Fixed::from_int(1000));
    assert!(pb.x > Fixed::from_int(1005));
    let gap = pb.x - pa.x;
    let min = units.get(a).unwrap().radius + units.get(b).unwrap().radius;
    assert!(gap >= min, "gap {gap:?} must reach {min:?}");
}

#[test]
fn a_structure_never_yields_in_a_collision() {
    let mut units = Arena::new();
    let tower = units.insert(Unit::tower(Team::Radiant, Vec2::from_ints(2000, 2000)));
    let creep = units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(2010, 2000),
    ));
    let tower_before = units.get(tower).unwrap().pos;
    crate::sim::separate_collisions(&mut units);
    assert_eq!(units.get(tower).unwrap().pos, tower_before);
    let gap = units.get(creep).unwrap().pos.x - tower_before.x;
    let min = units.get(tower).unwrap().radius + units.get(creep).unwrap().radius;
    assert!(gap >= min, "creep pushed clear of the tower");
}

#[test]
fn perfectly_stacked_units_still_separate() {
    let mut units = Arena::new();
    let a = units.insert(Unit::melee_creep(Team::Radiant, Vec2::from_ints(500, 500)));
    let b = units.insert(Unit::melee_creep(Team::Radiant, Vec2::from_ints(500, 500)));
    crate::sim::separate_collisions(&mut units);
    assert_ne!(units.get(a).unwrap().pos, units.get(b).unwrap().pos);
}
