//! Creep blocking: what a body in the way costs a wave.

use bota_proto::{Fixed, Team, Vec2};

use crate::sim::tests::fixtures::{aim_along_lane, hero_id, world};
use crate::sim::{Unit, UnitOrder, rules};

/// Marches one creep for `ticks` and reports how far along it got, with the
/// hero standing in its way, walking in front of it, or out of the picture.
fn progress(block: Block, ticks: u32) -> i64 {
    let start = Vec2::from_ints(8000, 8000);
    let mut w = world();
    for id in w.units.ids() {
        // No towers: this measures walking, not dying.
        if w.units.get(id).is_some_and(|u| u.is_structure()) {
            w.units.remove(id);
        }
    }
    let creep = w.units.insert(Unit::melee_creep(Team::Radiant, start));
    aim_along_lane(&mut w, creep, rules::LANE_MID);
    w.step(&[]);
    let UnitOrder::AttackMove { pos: heading } = w.units.get(creep).unwrap().order else {
        panic!("a lane creep marches its route");
    };
    let hero = hero_id(&w, 0);
    match block {
        Block::None => w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(1200, 1200),
        Block::Standing => {
            w.units.get_mut(hero).unwrap().pos =
                crate::sim::move_towards(start, heading, rules::units(36));
            w.units.get_mut(hero).unwrap().move_speed = Fixed::ZERO;
        }
        Block::Walking | Block::Mirroring => {
            w.units.get_mut(hero).unwrap().pos =
                crate::sim::move_towards(start, heading, rules::units(36));
            w.units.get_mut(hero).unwrap().move_speed = rules::units(300);
        }
    }
    if let Block::Walking = block {
        // It walks the same way under its own steam, a little slower, so the
        // creep keeps running into its back.
        w.units.get_mut(hero).unwrap().order = UnitOrder::Move { pos: heading };
    }
    for _ in 0..ticks {
        if let Block::Mirroring = block {
            // What a player does: keep putting the body back in front.
            let at = w.units.get(creep).unwrap().pos;
            w.units.get_mut(hero).unwrap().pos =
                crate::sim::move_towards(at, heading, rules::units(36));
        }
        w.step(&[]);
        if let Block::Walking = block {
            w.units.get_mut(hero).unwrap().order = UnitOrder::Move { pos: heading };
        }
    }
    crate::sim::isqrt64(w.units.get(creep).unwrap().pos.distance_squared(start))
}

enum Block {
    /// Nobody in the way.
    None,
    /// A body parked on the line and left there.
    Standing,
    /// A body walking the same way, a little slower.
    Walking,
    /// A player body-blocking: stepping back into the way every tick.
    Mirroring,
}

#[test]
fn a_body_in_the_way_costs_a_creep_ground() {
    let free = progress(Block::None, 400);
    for block in [Block::Standing, Block::Walking] {
        let blocked = progress(block, 400);
        assert!(
            blocked < free,
            "a body in the lane has to cost something: {blocked} against {free}"
        );
    }
}

#[test]
fn a_body_walked_in_front_costs_more_than_one_left_standing() {
    // Staying in the way is the thing: a body parked once is walked past.
    let standing = progress(Block::Standing, 400);
    let walking = progress(Block::Walking, 400);
    assert!(
        walking < standing,
        "keeping in front is worth more: {walking} against {standing}"
    );
}

#[test]
fn a_body_kept_in_front_stops_a_creep_almost_dead() {
    // One body walked past is worth little; a body put back in the way every
    // tick, which is what blocking a wave is, is worth nearly everything.
    let free = progress(Block::None, 400);
    let mirrored = progress(Block::Mirroring, 400);
    assert!(
        mirrored * 4 < free,
        "blocking has to be worth doing: {mirrored} against {free}"
    );
}

#[test]
fn a_creep_never_stalls_outright_against_a_body() {
    // Whatever the block costs, the wave keeps moving: no dead stops.
    let mut w = world();
    let start = Vec2::from_ints(8000, 8000);
    let creep = w.units.insert(Unit::melee_creep(Team::Radiant, start));
    aim_along_lane(&mut w, creep, rules::LANE_MID);
    w.step(&[]);
    let UnitOrder::AttackMove { pos: heading } = w.units.get(creep).unwrap().order else {
        panic!("a lane creep marches its route");
    };
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = crate::sim::move_towards(start, heading, rules::units(36));
    w.units.get_mut(hero).unwrap().move_speed = Fixed::ZERO;
    let mut still = 0;
    let mut worst = 0;
    let mut at = w.units.get(creep).unwrap().pos;
    for _ in 0..200 {
        w.step(&[]);
        let now = w.units.get(creep).unwrap().pos;
        if now == at {
            still += 1;
            worst = worst.max(still);
        } else {
            still = 0;
        }
        at = now;
    }
    assert!(
        worst < 12,
        "it works its way around rather than waiting: {worst} ticks of standing"
    );
}

/// The hulls the map is walked with, so a change to them is deliberate.
#[test]
fn walkers_carry_their_real_hulls() {
    let w = world();
    let melee = Unit::melee_creep(Team::Radiant, Vec2::ZERO);
    let ranged = Unit::ranged_creep(Team::Radiant, Vec2::ZERO);
    let siege = Unit::siege_creep(Team::Radiant, Vec2::ZERO);
    assert_eq!(melee.radius, rules::units(16));
    assert_eq!(ranged.radius, rules::units(8));
    assert_eq!(siege.radius, rules::units(16));
    let hero = w.units.get(hero_id(&w, 0)).unwrap();
    assert_eq!(hero.radius, rules::units(24));
}
