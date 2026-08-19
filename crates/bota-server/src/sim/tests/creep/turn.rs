//! Turning: how long a unit takes to come round, and at whose rate.

use bota_proto::{Angle, EntityId, Team, Vec2};

use crate::sim::tests::fixtures::mini_world;
use crate::sim::{NeutralKind, Unit, World, facing_gap, rules};

/// Puts `unit` on clear ground facing east with an enemy due west of it,
/// inside reach, and reports the world and the unit.
fn facing_east_with_a_mark_behind(build: impl Fn(Vec2) -> Unit) -> (World, EntityId) {
    let mut w = mini_world();
    let at = Vec2::from_ints(9000, 10400);
    let id = w.units.insert(build(at));
    let mark = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8900, 10400)));
    if let Some(u) = w.units.get_mut(id) {
        u.facing = Angle { brads: 0 };
        u.engage = Some(mark);
    }
    (w, id)
}

/// Ticks until `id` faces due west, turning no faster than its own rate.
fn ticks_to_come_round(w: &mut World, id: EntityId) -> u32 {
    let rate = w.units.get(id).expect("alive").turn_rate;
    let west = Angle { brads: 32768 };
    for t in 1..=60 {
        let before = w.units.get(id).expect("alive").facing;
        w.step(&[]);
        let after = w.units.get(id).expect("alive").facing;
        assert!(
            facing_gap(before, after) <= rate,
            "turned {} brads in one tick, rate is {rate}",
            facing_gap(before, after)
        );
        if after == west {
            return t;
        }
    }
    panic!("never came round");
}

#[test]
fn a_half_turn_takes_the_shipped_turn_rate() {
    let (mut w, creep) = facing_east_with_a_mark_behind(|at| Unit::melee_creep(Team::Radiant, at));
    // A half turn is 32768 brads. The shipped half a radian per 0.03 seconds
    // is 5795 brads over a tick of a thirtieth, so it takes six of them: a
    // fifth of a second, which is what Dota quotes for a turn rate of a half.
    assert_eq!(rules::TURN_RATE_BRADS, 5795);
    assert_eq!(ticks_to_come_round(&mut w, creep), 6);
}

#[test]
fn a_unit_comes_round_at_its_own_rate_not_the_creep_rate() {
    // A kobold turns at nine tenths where a lane creep turns at a half.
    let quick = NeutralKind::Kobold.def().turn_rate;
    assert!(
        quick > rules::TURN_RATE_BRADS,
        "the fixture needs a rate faster than a lane creep's"
    );
    let (mut w, creep) = facing_east_with_a_mark_behind(|at| Unit::melee_creep(Team::Radiant, at));
    let (mut q, quicker) = facing_east_with_a_mark_behind(|at| Unit {
        turn_rate: quick,
        ..Unit::melee_creep(Team::Radiant, at)
    });
    assert!(
        ticks_to_come_round(&mut q, quicker) < ticks_to_come_round(&mut w, creep),
        "the faster unit has to come round sooner"
    );
}
