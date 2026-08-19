//! The wave schedule, its composition and its upgrades.

use bota_proto::{Team, UnitKind};

use crate::sim::tests::fixtures::world;
use crate::sim::{rules, wave_at, wave_plan, wave_upgrades};

#[test]
fn waves_are_numbered_from_the_horn_every_thirty_seconds() {
    assert_eq!(wave_at(rules::FIRST_WAVE_TICK - 1), None);
    assert_eq!(wave_at(rules::FIRST_WAVE_TICK), Some(1));
    assert_eq!(wave_at(rules::FIRST_WAVE_TICK + 1), None);
    assert_eq!(
        wave_at(rules::FIRST_WAVE_TICK + rules::WAVE_PERIOD_TICKS),
        Some(2)
    );
    // Wave 11 is five minutes in, wave 31 is fifteen.
    assert_eq!(
        wave_at(rules::FIRST_WAVE_TICK + 10 * rules::WAVE_PERIOD_TICKS),
        Some(11)
    );
    assert_eq!(
        wave_at(rules::FIRST_WAVE_TICK + 30 * rules::WAVE_PERIOD_TICKS),
        Some(31)
    );
}

#[test]
fn the_opening_wave_is_three_melee_and_one_ranged() {
    let plan = wave_plan(1);
    assert_eq!((plan.melee, plan.ranged, plan.siege), (3, 1, 0));
    assert!(!plan.flagbearer);
    assert_eq!(plan.upgrades, 0);
    assert_eq!(plan.size(), 4);
}

#[test]
fn the_flagbearer_joins_on_the_fifth_wave_and_every_second_after() {
    for wave in 1..=4 {
        assert!(!wave_plan(wave).flagbearer, "none before the fifth");
    }
    for wave in [5, 7, 9, 11, 41] {
        assert!(wave_plan(wave).flagbearer, "wave {wave} carries one");
    }
    for wave in [6, 8, 10, 40] {
        assert!(!wave_plan(wave).flagbearer, "wave {wave} does not");
    }
    // It replaces a melee creep rather than joining them.
    assert_eq!(wave_plan(5).melee, wave_plan(4).melee);
    assert_eq!(wave_plan(5).size(), wave_plan(4).size());
}

#[test]
fn the_siege_creep_joins_on_the_eleventh_wave_and_every_tenth_after() {
    for wave in 1..=10 {
        assert_eq!(wave_plan(wave).siege, 0, "none before the eleventh");
    }
    for wave in [11, 21, 31, 41] {
        assert_eq!(wave_plan(wave).siege, 1, "wave {wave} brings one");
    }
    for wave in [12, 20, 30] {
        assert_eq!(wave_plan(wave).siege, 0, "wave {wave} brings none");
    }
    // A second one from wave 71, on the same every-tenth cadence.
    assert_eq!(wave_plan(71).siege, 2);
    assert_eq!(wave_plan(81).siege, 2);
    assert_eq!(wave_plan(72).siege, 0);
}

#[test]
fn the_count_grows_on_the_published_waves() {
    let melee = |w| wave_plan(w).melee;
    assert_eq!((melee(30), melee(31)), (3, 4), "fifteen minutes");
    assert_eq!((melee(60), melee(61)), (4, 5), "thirty minutes");
    assert_eq!((melee(90), melee(91)), (5, 6), "forty five minutes");
    let ranged = |w| wave_plan(w).ranged;
    assert_eq!((ranged(80), ranged(81)), (1, 2), "forty minutes");
}

#[test]
fn upgrades_arrive_every_fifteen_waves_and_stop_at_thirty() {
    assert_eq!(wave_upgrades(1), 0);
    assert_eq!(wave_upgrades(15), 0);
    assert_eq!(wave_upgrades(16), 1, "seven and a half minutes");
    assert_eq!(wave_upgrades(31), 2);
    assert_eq!(wave_upgrades(451), 30, "the cap, at 225 minutes");
    assert_eq!(wave_upgrades(1000), 30, "and it stays there");
}

#[test]
fn an_upgraded_wave_carries_the_published_stats() {
    let mut w = world();
    // Wave 16, the first upgraded one.
    w.tick = rules::FIRST_WAVE_TICK + 15 * rules::WAVE_PERIOD_TICKS;
    w.spawn_waves();
    let melee = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::CreepMelee && u.team == Team::Radiant)
        .map(|(_, u)| u.clone())
        .expect("a melee creep spawned");
    assert_eq!(
        melee.max_hp,
        rules::MELEE_CREEP_HP + rules::MELEE_UPGRADE_HP
    );
    assert_eq!(
        melee.attack_damage,
        rules::MELEE_CREEP_ATTACK_DAMAGE + rules::MELEE_UPGRADE_DAMAGE
    );
    assert_eq!(
        melee.bounty,
        rules::MELEE_CREEP_BOUNTY + rules::MELEE_UPGRADE_GOLD
    );
    let ranged = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::CreepRanged && u.team == Team::Radiant)
        .map(|(_, u)| u.clone())
        .expect("a ranged creep spawned");
    assert_eq!(
        ranged.max_hp,
        rules::RANGED_CREEP_HP + rules::RANGED_UPGRADE_HP
    );
    assert_eq!(
        ranged.xp_reward,
        rules::RANGED_CREEP_XP + rules::RANGED_UPGRADE_XP
    );
}

#[test]
fn a_flagbearer_wave_spawns_one_per_lane_and_side() {
    let mut w = world();
    w.tick = rules::FIRST_WAVE_TICK + 4 * rules::WAVE_PERIOD_TICKS; // wave 5
    w.spawn_waves();
    let flags = w
        .units
        .iter()
        .filter(|(_, u)| u.kind == UnitKind::CreepFlagbearer)
        .count();
    assert_eq!(flags, 6, "one per lane per side");
    let melee = w
        .units
        .iter()
        .filter(|(_, u)| u.kind == UnitKind::CreepMelee)
        .count();
    assert_eq!(melee, 12, "and it took a melee creep's place");
    let flag = w
        .units
        .iter()
        .find(|(_, u)| u.kind == UnitKind::CreepFlagbearer)
        .map(|(_, u)| u.clone())
        .unwrap();
    assert_eq!(flag.magic_resist_pct, rules::FLAGBEARER_MAGIC_RESIST_PCT);
    assert_eq!(flag.max_hp, rules::MELEE_CREEP_HP, "and takes no upgrades");
}

#[test]
fn a_siege_wave_puts_the_ranged_creep_behind_the_front_rank() {
    let mut w = world();
    w.tick = rules::FIRST_WAVE_TICK + 10 * rules::WAVE_PERIOD_TICKS; // wave 11
    w.spawn_waves();
    let spawn = rules::RADIANT_CREEP_SPAWNS[usize::from(rules::LANE_MID)];
    let route = crate::sim::lane_route(w.map, Team::Radiant, rules::LANE_MID);
    let ahead = route[0] - spawn;
    let dot = |p: bota_proto::Vec2| {
        let d = p - spawn;
        i64::from(d.x.raw) * i64::from(ahead.x.raw) + i64::from(d.y.raw) * i64::from(ahead.y.raw)
    };
    let ranged = w
        .units
        .iter()
        .filter(|(_, u)| {
            u.kind == UnitKind::CreepRanged && u.team == Team::Radiant && u.lane == rules::LANE_MID
        })
        .map(|(_, u)| dot(u.pos))
        .min()
        .unwrap();
    let front = w
        .units
        .iter()
        .filter(|(_, u)| {
            u.team == Team::Radiant
                && u.lane == rules::LANE_MID
                && matches!(
                    u.kind,
                    UnitKind::CreepMelee | UnitKind::CreepFlagbearer | UnitKind::CreepSiege
                )
        })
        .map(|(_, u)| dot(u.pos))
        .min()
        .unwrap();
    assert!(
        ranged < front,
        "the ranged rank trails the melee one: {ranged} vs {front}"
    );
}
