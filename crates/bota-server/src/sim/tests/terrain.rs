//! Elevation tiers, the river, highground vision and uphill misses.

use bota_proto::{EventKind, Fixed, Order, SlotId, Team, Vec2};

use super::fixtures::{hero_id, world};
use crate::sim::{Command, Unit, World, rules};

fn cmd(slot: u8, order: Order) -> Command {
    Command {
        slot: SlotId(slot),
        order,
    }
}

fn cell_center(cx: i32, cy: i32) -> Vec2 {
    Vec2::from_ints(
        cx * rules::GRID_CELL_SIZE + rules::GRID_CELL_SIZE / 2,
        cy * rules::GRID_CELL_SIZE + rules::GRID_CELL_SIZE / 2,
    )
}

/// A walkable spot and a walkable spot `gap` cells east one tier higher,
/// both out of sight and reach of every standing unit.
fn ramp_pair(w: &World, gap: i32) -> (Vec2, Vec2) {
    for cy in 30..260 {
        for cx in 30..260 {
            let low = cell_center(cx, cy);
            let high = cell_center(cx + gap, cy);
            if !w.grid.walkable(low) || !w.grid.walkable(high) {
                continue;
            }
            if w.ground.tier(high) != w.ground.tier(low) + 1 {
                continue;
            }
            let clear = w.units.iter().all(|(_, u)| {
                let reach = u.vision_radius.max(u.attack_range) + rules::units(600);
                !u.pos.within(low, reach) && !u.pos.within(high, reach)
            });
            if clear {
                return (low, high);
            }
        }
    }
    panic!("no ramp found");
}

#[test]
fn the_ground_knows_its_landmarks() {
    let w = world();
    // The map's own heights: tier-one lane ground by the river, tier-two
    // highground at the tier-three tower, tier-three base at the fountain.
    assert_eq!(
        w.ground.tier(Vec2::from_ints(7672, 7808)),
        1,
        "mid tier one"
    );
    assert_eq!(
        w.ground.tier(Vec2::from_ints(4576, 5072)),
        2,
        "mid tier three stands on highground"
    );
    assert_eq!(
        w.ground.tier(rules::RADIANT_FOUNTAIN_POS),
        3,
        "the base is the top tier"
    );
}

#[test]
fn the_river_runs_wide() {
    let w = world();
    let mut wet = 0;
    for cy in 0..rules::GRID_CELLS as i32 {
        for cx in 0..rules::GRID_CELLS as i32 {
            if w.ground.water(cell_center(cx, cy)) {
                wet += 1;
            }
        }
    }
    assert!(
        wet > 10_000,
        "the river and pools cover ground: {wet} cells"
    );
}

#[test]
fn lowground_does_not_see_highground() {
    let mut w = world();
    let (low, high) = ramp_pair(&world(), 3);
    let dire = hero_id(&w, 1);
    w.units.get_mut(dire).unwrap().pos = low;
    assert!(
        !w.can_see_point(Team::Dire, high),
        "the higher ground is dark from below"
    );
    assert!(
        w.can_see_point(Team::Dire, low),
        "the ground underfoot is lit"
    );
    // From the top the low ground is plainly visible.
    w.units.get_mut(dire).unwrap().pos = high;
    assert!(w.can_see_point(Team::Dire, low));
}

#[test]
fn the_roshan_pit_is_dark_from_outside_and_lit_from_within() {
    let mut w = world();
    let dire = hero_id(&w, 1);
    // A ring of lookouts around the pit, thirty degrees apart: nobody sees
    // in, the entrance included — the map's own fog blockers seal it.
    let ring = [
        (900, 0),
        (780, 450),
        (450, 780),
        (0, 900),
        (-450, 780),
        (-780, 450),
        (-900, 0),
        (-780, -450),
        (-450, -780),
        (0, -900),
        (450, -780),
        (780, -450),
    ];
    for (dx, dy) in ring {
        let spot = rules::ROSHAN_PIT + Vec2::from_ints(dx, dy);
        w.units.get_mut(dire).unwrap().pos = spot;
        assert!(
            !w.can_see_point(Team::Dire, rules::ROSHAN_PIT),
            "the pit is dark from {spot:?}"
        );
    }
    // One step inside and it is all in plain sight.
    w.units.get_mut(dire).unwrap().pos = rules::ROSHAN_PIT + Vec2::from_ints(100, 0);
    assert!(w.can_see_point(Team::Dire, rules::ROSHAN_PIT));
}

#[test]
fn the_river_is_visible_across_the_mid_crossing() {
    let mut w = world();
    let dire = hero_id(&w, 1);
    // One bank of the mid river crossing sees the other; no phantom wall
    // between the two Roshan pits cuts the river in half.
    w.units.get_mut(dire).unwrap().pos = Vec2::from_ints(9100, 9300);
    assert!(
        w.can_see_point(Team::Dire, Vec2::from_ints(8300, 8400)),
        "the far side of the crossing is lit"
    );
}

#[test]
fn a_tree_shades_what_hides_behind_it() {
    let mut w = world();
    let dire = hero_id(&w, 1);
    // A tree with open, level ground on both sides, out of everyone's way.
    let mut found = None;
    'trees: for tree in crate::sim::tree_positions() {
        let front = tree + Vec2::from_ints(220, 0);
        let behind = tree - Vec2::from_ints(220, 0);
        if !w.grid.walkable(front) || !w.grid.walkable(behind) {
            continue;
        }
        if w.ground.tier(front) != w.ground.tier(tree)
            || w.ground.tier(behind) != w.ground.tier(tree)
        {
            continue;
        }
        for (_, u) in w.units.iter() {
            if u.vision_radius > bota_proto::Fixed::ZERO
                && u.team == Team::Dire
                && u.pos.within(tree, u.vision_radius + rules::units(300))
            {
                continue 'trees;
            }
        }
        found = Some((front, behind));
        break;
    }
    let (front, behind) = found.expect("such a tree exists");
    w.units.get_mut(dire).unwrap().pos = front;
    assert!(
        !w.can_see_point(Team::Dire, behind),
        "the far side of the trunk is dark"
    );
    assert!(
        w.can_see_point(Team::Dire, front),
        "the ground underfoot is lit"
    );
}

#[test]
fn ranged_attacks_miss_uphill_now_and_then() {
    let mut w = world();
    let (low, high) = ramp_pair(&world(), 5);
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = low;
    w.units.get_mut(hero).unwrap().max_hp = 100_000;
    w.units.get_mut(hero).unwrap().hp = 100_000;
    let dummy = w.units.insert(Unit::melee_creep(Team::Dire, high));
    w.units.get_mut(dummy).unwrap().move_speed = Fixed::ZERO;
    w.units.get_mut(dummy).unwrap().max_hp = 1_000_000;
    w.units.get_mut(dummy).unwrap().hp = 1_000_000;
    w.step(&[cmd(0, Order::AttackUnit { target: dummy })]);
    let mut hits = 0;
    for _ in 0..3000 {
        for e in w.step(&[]) {
            if let EventKind::Damaged {
                source: Some(s),
                target,
                ..
            } = e.kind
                && s == hero
                && target == dummy
            {
                hits += 1;
            }
        }
    }
    let swings = 3000 / rules::HERO_ATTACK_INTERVAL;
    assert!(hits > swings / 2, "most attacks land: {hits} of ~{swings}");
    assert!(
        hits < swings - 3,
        "but a quarter go wide uphill: {hits} of ~{swings}"
    );
}
