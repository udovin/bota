//! Bodies that end up inside one another come apart again.

use bota_proto::{Team, Vec2};

use crate::sim::tests::fixtures::mini_world;
use crate::sim::{Unit, rules};

#[test]
fn two_creeps_stacked_on_one_spot_come_apart() {
    let mut w = mini_world();
    let at = Vec2::from_ints(9000, 9216);
    let a = w.units.insert(Unit::melee_creep(Team::Radiant, at));
    let b = w.units.insert(Unit::melee_creep(Team::Radiant, at));
    let hulls =
        w.units.get(a).expect("just spawned").radius + w.units.get(b).expect("just spawned").radius;
    for _ in 0..30 {
        w.step(&[]);
    }
    let (a, b) = (
        w.units.get(a).expect("nothing killed it").pos,
        w.units.get(b).expect("nothing killed it").pos,
    );
    assert!(
        !a.within(b, hulls),
        "still inside one another: {a:?} and {b:?}"
    );
}

#[test]
fn a_creep_in_a_tower_is_the_one_that_moves() {
    let mut w = mini_world();
    let tower = w
        .units
        .ids()
        .into_iter()
        .find(|id| {
            w.units
                .get(*id)
                .is_some_and(|u| u.team == Team::Dire && u.pos.x.to_int() == 10800)
        })
        .expect("the mini map stands a Dire tier one");
    let stood = w.units.get(tower).expect("just found it").pos;
    let creep = w.units.insert(Unit::melee_creep(Team::Radiant, stood));
    w.step(&[]);
    assert_eq!(w.units.get(tower).expect("towers do not walk").pos, stood);
    assert_ne!(
        w.units.get(creep).expect("one tick cannot kill it").pos,
        stood
    );
}

#[test]
fn a_body_is_eased_out_no_faster_than_one_step_a_tick() {
    let mut w = mini_world();
    let at = Vec2::from_ints(9000, 9216);
    let a = w.units.insert(Unit::melee_creep(Team::Radiant, at));
    let _b = w.units.insert(Unit::melee_creep(Team::Radiant, at));
    w.step(&[]);
    let moved = w.units.get(a).expect("nothing killed it").pos;
    // One separation step, plus at most one step of its own march.
    let most = rules::units(rules::SEPARATION_STEP) + rules::units(rules::CREEP_MOVE_SPEED);
    assert!(
        moved.within(at, most),
        "moved {moved:?} from {at:?} in a tick"
    );
}

#[test]
fn a_creep_pinned_between_a_tower_and_a_body_works_its_way_out() {
    let mut w = mini_world();
    // The gap beside the tier one is shut: the tower closes the ground on one
    // side, a hero stands on the other.
    let hero = crate::sim::tests::fixtures::hero_id(&w, 0);
    if let Some(u) = w.units.get_mut(hero) {
        u.pos = Vec2::from_ints(9560, 9296);
    }
    let creep = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(9528, 9256),
    ));
    crate::sim::tests::fixtures::aim_along_lane(&mut w, creep, rules::LANE_MID);
    let start = w.units.get(creep).expect("just spawned").pos;
    for _ in 0..300 {
        w.step(&[]);
    }
    let now = w.units.get(creep).expect("nothing can kill it here").pos;
    assert!(
        !now.within(start, bota_proto::Fixed::from_int(200)),
        "still pinned: {now:?} against {start:?}"
    );
}
