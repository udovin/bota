//! Lane creep behaviour: the chase, the way back, the route.

use bota_proto::{Fixed, Team, Vec2};

use crate::sim::tests::fixtures::{aim_along_lane, hero_id, mini_world, world};
use crate::sim::{CreepAi, Unit, UnitOrder, World, rules};

fn step_n(w: &mut World, n: u32) {
    for _ in 0..n {
        w.step(&[]);
    }
}

fn lane_ai(w: &World, id: bota_proto::EntityId) -> crate::sim::LaneCreepAi {
    match w.units.get(id).unwrap().ai.clone() {
        Some(CreepAi::Lane(ai)) => ai,
        _ => panic!("a lane creep carries a lane ai"),
    }
}

/// A creep on the mid lane, already past its own towers, with a bait hero
/// standing next to it. Nothing else is near enough to interfere.
fn baited() -> (World, bota_proto::EntityId, bota_proto::EntityId) {
    let mut w = world();
    let creep = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8300, 8300)));
    aim_along_lane(&mut w, creep, rules::LANE_MID);
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8700, 8300);
    w.units.get_mut(hero).unwrap().move_speed = Fixed::ZERO;
    // Tough enough to survive the whole probe: an invulnerable bait would
    // not be a target at all.
    w.units.get_mut(hero).unwrap().hp = 1_000_000;
    w.units.get_mut(hero).unwrap().max_hp = 1_000_000;
    (w, creep, hero)
}

#[test]
fn leaving_the_route_records_the_spot_to_come_back_to() {
    let (mut w, creep, hero) = baited();
    assert_eq!(lane_ai(&w, creep).anchor, None, "on the route, no anchor");
    w.step(&[]);
    let ai = lane_ai(&w, creep);
    assert_eq!(w.units.get(creep).unwrap().engage, Some(hero));
    assert_eq!(
        ai.anchor,
        Some(Vec2::from_ints(8300, 8300)),
        "the anchor is where it left, not the nearest lane point"
    );
}

#[test]
fn a_target_out_of_acquisition_range_is_chased_for_two_and_a_bit_seconds() {
    let (mut w, creep, hero) = baited();
    w.step(&[]);
    assert_eq!(w.units.get(creep).unwrap().engage, Some(hero));
    // The bait steps outside acquisition range but stays in plain sight.
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(9600, 8300);
    w.units.get_mut(creep).unwrap().move_speed = Fixed::ZERO;
    step_n(&mut w, rules::CREEP_CHASE_TICKS - 1);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        Some(hero),
        "still chasing inside the window"
    );
    step_n(&mut w, 2);
    assert_eq!(
        w.units.get(creep).unwrap().engage,
        None,
        "the chase is spent"
    );
}

#[test]
fn a_creep_that_gave_up_walks_back_to_where_it_left() {
    let (mut w, creep, hero) = baited();
    w.step(&[]);
    let anchor = lane_ai(&w, creep).anchor.expect("anchor set");
    // Drag it well off the route, then take the bait away.
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8300, 9400);
    step_n(&mut w, 90);
    let away = w.units.get(creep).unwrap().pos;
    assert!(
        !away.within(anchor, rules::units(200)),
        "it followed the bait off the route, at {away:?}"
    );
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(1200, 1200);
    step_n(&mut w, 3);
    assert_eq!(w.units.get(creep).unwrap().engage, None);
    assert_eq!(
        w.units.get(creep).unwrap().order,
        UnitOrder::AttackMove { pos: anchor },
        "it heads for the spot it left, not the nearest lane point"
    );
    // It is back on its lane that counts: the anchor sits on the lane, so
    // rejoining and arriving are the same moment.
    let mut home = false;
    for _ in 0..200 {
        w.step(&[]);
        let c = w.units.get(creep).unwrap();
        if crate::sim::lane_offset_squared(w.map, rules::LANE_MID, c.pos)
            <= rules::units(rules::LANE_WAYPOINT_RADIUS).squared_raw()
        {
            home = true;
            break;
        }
    }
    assert!(home, "and it gets back onto its lane");
}

#[test]
fn a_target_lost_to_fog_is_followed_to_the_last_spot() {
    let (mut w, creep, hero) = baited();
    w.step(&[]);
    let seen_at = Vec2::from_ints(8700, 8300);
    // The hero blinks across the map: no Dire eye is left on it.
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(1200, 1200);
    w.step(&[]);
    let ai = lane_ai(&w, creep);
    assert_eq!(w.units.get(creep).unwrap().engage, None, "lost to the fog");
    assert_eq!(ai.last_seen, Some(seen_at));
    assert_eq!(
        w.units.get(creep).unwrap().order,
        UnitOrder::AttackMove { pos: seen_at },
        "it walks up to the last spot it saw"
    );
}

#[test]
fn a_creep_marches_the_walked_route_of_its_own_lane() {
    let mut w = world();
    let creep = w.units.insert(Unit::melee_creep(
        Team::Radiant,
        Vec2::from_ints(8000, 8000),
    ));
    aim_along_lane(&mut w, creep, rules::LANE_MID);
    let route = crate::sim::lane_routes(w.map)[crate::sim::team_index(Team::Radiant)]
        [usize::from(rules::LANE_MID)]
    .clone();
    w.step(&[]);
    let step = lane_ai(&w, creep).step;
    assert_eq!(
        w.units.get(creep).unwrap().order,
        UnitOrder::AttackMove {
            pos: route[usize::from(step)]
        },
        "it pushes towards its own next waypoint"
    );
    // Every waypoint of the route stands on ground it can walk.
    for point in &route {
        assert!(w.grid.walkable(*point), "waypoint {point:?} is walkable");
    }
    // And the route ends beside the enemy Ancient, which closes its own cell.
    assert!(
        route.last().unwrap().within(
            crate::sim::ancient_pos(w.map, Team::Dire),
            rules::units(200)
        ),
        "the march ends at the enemy base, at {:?}",
        route.last()
    );
}

#[test]
fn a_walked_route_keeps_to_its_own_lane() {
    let w = world();
    for team in [Team::Radiant, Team::Dire] {
        for lane in [rules::LANE_MID, rules::LANE_TOP, rules::LANE_BOT] {
            let route =
                &crate::sim::lane_routes(w.map)[crate::sim::team_index(team)][usize::from(lane)];
            for point in route {
                let off = crate::sim::lane_offset_squared(w.map, lane, *point);
                assert!(
                    off <= rules::units(900).squared_raw(),
                    "{team:?} lane {lane} strays at {point:?}"
                );
            }
        }
    }
}

#[test]
fn a_flagbearer_marches_like_the_melee_creep_it_is() {
    let mut w = mini_world();
    let at = Vec2::from_ints(8000, 9216);
    let flag = w.units.insert(Unit::flagbearer_creep(Team::Radiant, at));
    aim_along_lane(&mut w, flag, rules::LANE_MID);
    for _ in 0..200 {
        w.step(&[]);
    }
    let now = w.units.get(flag).expect("nothing kills it here").pos;
    assert!(
        now.x.to_int() > at.x.to_int() + 400,
        "a flagbearer has to walk its lane: {now:?} from {at:?}"
    );
}

#[test]
fn a_flagbearer_picks_a_target_like_the_melee_creep_it_is() {
    let mut w = mini_world();
    let flag = w.units.insert(Unit::flagbearer_creep(
        Team::Radiant,
        Vec2::from_ints(8000, 9216),
    ));
    aim_along_lane(&mut w, flag, rules::LANE_MID);
    let mark = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8300, 9216)));
    w.step(&[]);
    assert_eq!(
        w.units.get(flag).expect("just spawned").engage,
        Some(mark),
        "a flagbearer has to take a target in reach"
    );
}
