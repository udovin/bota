//! Entity allocation and component storage.

use bota_proto::{Fixed, Team};

use crate::engine::{
    AbilityBook, AbilityState, Def, Entity, EntityAllocator, FLAGBEARER_CREEP, HERO, Health,
    Inventory, ItemStack, Level, MELEE_CREEP, Mana, NEUTRALS, NeutralKind, RANGED_CREEP, Stats,
    Status, StatusKind, Statuses, Table, Upgrades, Visibility, World,
};
use crate::sim::rules;

#[test]
fn an_allocator_counts_what_is_live() {
    let mut entities = EntityAllocator::new();
    assert!(entities.is_empty());
    let first = entities.alloc();
    let second = entities.alloc();
    assert_eq!(entities.len(), 2);
    assert!(entities.contains(first) && entities.contains(second));
    assert!(entities.free(first));
    assert_eq!(entities.len(), 1);
    assert!(!entities.is_empty());
}

#[test]
fn a_handle_kept_past_a_death_names_nobody() {
    let mut entities = EntityAllocator::new();
    let dead = entities.alloc();
    assert!(entities.free(dead));
    assert!(!entities.contains(dead));
    assert!(!entities.free(dead), "freeing twice has to be refused");
}

#[test]
fn a_slot_handed_out_again_carries_a_raised_generation() {
    let mut entities = EntityAllocator::new();
    let first = entities.alloc();
    assert!(entities.free(first));
    let second = entities.alloc();
    assert_eq!(first.index(), second.index(), "the free slot is taken back");
    assert_ne!(first.generation(), second.generation());
    assert!(!entities.contains(first) && entities.contains(second));
}

#[test]
fn live_entities_come_out_in_slot_order() {
    let mut entities = EntityAllocator::new();
    let all: Vec<Entity> = (0..4).map(|_| entities.alloc()).collect();
    assert!(entities.free(all[1]));
    let live: Vec<Entity> = entities.iter().collect();
    assert_eq!(live, vec![all[0], all[2], all[3]]);
    let fresh = entities.alloc();
    let live: Vec<Entity> = entities.iter().collect();
    assert_eq!(
        live,
        vec![all[0], fresh, all[2], all[3]],
        "a reused slot is walked where it sits, not where it was made"
    );
}

#[test]
fn a_table_holds_a_component_for_the_entity_that_owns_it() {
    let mut entities = EntityAllocator::new();
    let mine = entities.alloc();
    let theirs = entities.alloc();
    let mut table: Table<i32> = Table::new();
    assert_eq!(table.insert(mine, 7), None);
    assert_eq!(table.get(mine), Some(&7));
    assert!(!table.contains(theirs));
    assert_eq!(table.insert(mine, 9), Some(7), "the old value comes back");
    *table.get_mut(mine).expect("just written") += 1;
    assert_eq!(table.get(mine), Some(&10));
    assert_eq!(table.remove(mine), Some(10));
    assert!(!table.contains(mine));
    assert_eq!(table.remove(mine), None);
}

#[test]
fn what_a_dead_entity_left_is_never_the_new_tenants() {
    let mut entities = EntityAllocator::new();
    let first = entities.alloc();
    let mut table: Table<i32> = Table::new();
    table.insert(first, 7);
    assert!(entities.free(first));
    let second = entities.alloc();
    assert_eq!(first.index(), second.index());
    assert_eq!(table.get(second), None, "the slot came empty");
    assert_eq!(
        table.insert(second, 3),
        None,
        "nothing of its own to return"
    );
    assert_eq!(table.get(second), Some(&3));
    assert_eq!(table.get(first), None, "the old handle reads nothing");
}

#[test]
fn a_table_answers_for_the_handle_it_is_given_not_for_the_living() {
    // Liveness is the allocator's to know. Until the slot changes hands the
    // table still answers the dead handle, so walking entities goes through
    // `EntityAllocator::iter` and never through a table alone.
    let mut entities = EntityAllocator::new();
    let gone = entities.alloc();
    let mut table: Table<i32> = Table::new();
    table.insert(gone, 7);
    assert!(entities.free(gone));
    assert!(!entities.contains(gone));
    assert_eq!(table.get(gone), Some(&7));
}

/// A stat block with every field named, so a new one has to be thought about
/// here before any test compiles again.
fn stats() -> Stats {
    Stats {
        max_hp: Fixed::from_int(20),
        max_mana: Fixed::from_int(20),
        hp_regen: Fixed::ZERO,
        mana_regen: Fixed::ZERO,
        damage: 0,
        attack_range: Fixed::ZERO,
        acquisition: Fixed::ZERO,
        attack_interval: 30,
        attack_point: 0,
        attack_backswing: 0,
        projectile_speed: None,
        armor: 0,
        magic_resist_pct: 0,
        move_speed: Fixed::ZERO,
        turn_rate: 0,
        vision: Fixed::ZERO,
        invulnerable: false,
    }
}

/// A pool holding a whole number of points.
fn health(hp: i32) -> Health {
    Health {
        hp: Fixed::from_int(hp),
    }
}

#[test]
fn a_tick_mends_what_can_mend() {
    let mut world = World::new();
    let hurt = world.spawn();
    world.health.insert(hurt, health(10));
    world.stats.insert(
        hurt,
        Stats {
            hp_regen: Fixed::from_int(3),
            ..stats()
        },
    );
    world.step();
    assert_eq!(world.tick, 1);
    assert_eq!(world.health.get(hurt).map(|h| h.hp.to_int()), Some(13));
    for _ in 0..10 {
        world.step();
    }
    assert_eq!(
        world.health.get(hurt).map(|h| h.hp.to_int()),
        Some(20),
        "mending stops at the maximum"
    );
}

#[test]
fn mending_finer_than_a_point_gathers_until_it_is_worth_one() {
    let mut world = World::new();
    let hurt = world.spawn();
    world.health.insert(hurt, health(10));
    world.stats.insert(
        hurt,
        Stats {
            hp_regen: Fixed::from_ratio(1, 4),
            ..stats()
        },
    );
    for _ in 0..3 {
        world.step();
    }
    assert_eq!(
        world.health.get(hurt).map(|h| h.hp.to_int()),
        Some(10),
        "three quarters of a point is not a point"
    );
    world.step();
    assert_eq!(world.health.get(hurt).map(|h| h.hp.to_int()), Some(11));
    for _ in 0..4 {
        world.step();
    }
    assert_eq!(
        world.health.get(hurt).map(|h| h.hp.to_int()),
        Some(12),
        "nothing is lost between whole points"
    );
}

#[test]
fn mana_mends_the_same_way() {
    let mut world = World::new();
    let caster = world.spawn();
    world.mana.insert(caster, Mana { mana: Fixed::ZERO });
    world.stats.insert(
        caster,
        Stats {
            mana_regen: Fixed::from_ratio(1, 2),
            ..stats()
        },
    );
    for _ in 0..2 {
        world.step();
    }
    assert_eq!(world.mana.get(caster).map(|m| m.mana.to_int()), Some(1));
}

#[test]
fn the_dead_mend_nothing() {
    let mut world = World::new();
    let dead = world.spawn();
    world.health.insert(dead, health(0));
    world.stats.insert(
        dead,
        Stats {
            hp_regen: Fixed::from_int(3),
            ..stats()
        },
    );
    world.step();
    assert_eq!(world.health.get(dead).map(|h| h.hp.to_int()), Some(0));
}

#[test]
fn a_tick_leaves_alone_what_carries_no_stats() {
    let mut world = World::new();
    let stone = world.spawn();
    world.health.insert(stone, health(10));
    world.step();
    assert_eq!(world.health.get(stone).map(|h| h.hp.to_int()), Some(10));
}

#[test]
fn a_tick_leaves_alone_what_is_no_longer_in_the_world() {
    let mut world = World::new();
    let gone = world.spawn();
    world.health.insert(gone, health(10));
    world.stats.insert(
        gone,
        Stats {
            hp_regen: Fixed::from_int(3),
            ..stats()
        },
    );
    assert!(world.despawn(gone));
    world.step();
    // The row is still there, untouched: the tick walks the allocator, and
    // the allocator no longer names it.
    assert_eq!(world.health.get(gone).map(|h| h.hp.to_int()), Some(10));
}

#[test]
fn what_a_despawned_entity_left_is_not_inherited() {
    let mut world = World::new();
    let first = world.spawn();
    world.health.insert(first, health(10));
    world.stats.insert(
        first,
        Stats {
            hp_regen: Fixed::from_int(3),
            ..stats()
        },
    );
    assert!(world.despawn(first));
    let second = world.spawn();
    assert_eq!(first.index(), second.index());
    assert_eq!(world.health.get(second), None);
    world.step();
    assert_eq!(
        world.health.get(second),
        None,
        "it mends nothing it never had"
    );
}

#[test]
fn who_sees_an_entity_is_a_set_of_bits() {
    let mut seen = Visibility::default();
    assert!(!seen.by(Team::Radiant));
    seen.add(Team::Radiant);
    seen.add(Team::Radiant);
    assert!(seen.by(Team::Radiant), "naming a side twice sets one bit");
    assert!(!seen.by(Team::Dire), "and leaves the others alone");
    seen.add(Team::Dire);
    assert!(seen.by(Team::Radiant) && seen.by(Team::Dire));
    seen.clear();
    assert!(seen.is_empty() && !seen.by(Team::Dire));
}

/// A creep with nothing done to it.
fn plain_creep(world: &mut World) -> Entity {
    let creep = world.spawn();
    world.def.insert(creep, Def(&MELEE_CREEP));
    world.health.insert(creep, Health { hp: Fixed::ZERO });
    creep
}

#[test]
fn a_plain_creep_gets_the_numbers_of_its_kind() {
    let mut world = World::new();
    let creep = plain_creep(&mut world);
    world.step();
    let stats = world.stats.get(creep).expect("worked out this tick");
    assert_eq!(stats.max_hp, Fixed::from_int(rules::MELEE_CREEP_HP));
    assert_eq!(stats.damage, rules::MELEE_CREEP_ATTACK_DAMAGE);
    assert_eq!(stats.armor, rules::MELEE_CREEP_ARMOR);
    assert_eq!(stats.attack_interval, rules::CREEP_ATTACK_INTERVAL);
    assert_eq!(stats.projectile_speed, None, "a melee creep throws nothing");
}

#[test]
fn a_pool_is_capped_but_never_filled_by_working_out_stats() {
    let mut world = World::new();
    let hurt = plain_creep(&mut world);
    world.health.insert(
        hurt,
        Health {
            hp: Fixed::from_int(100),
        },
    );
    let brimming = plain_creep(&mut world);
    world.health.insert(
        brimming,
        Health {
            hp: Fixed::from_int(rules::MELEE_CREEP_HP + 400),
        },
    );
    world.step();
    assert_eq!(
        world.health.get(hurt).map(|h| h.hp),
        Some(Fixed::from_int(100)),
        "what an entity spawns with is left alone"
    );
    assert_eq!(
        world.health.get(brimming).map(|h| h.hp),
        Some(Fixed::from_int(rules::MELEE_CREEP_HP)),
        "over the maximum is brought down to it"
    );
}

#[test]
fn upgrades_raise_a_creep_and_carry_its_health_with_them() {
    let mut world = World::new();
    let creep = plain_creep(&mut world);
    world.health.insert(
        creep,
        Health {
            hp: Fixed::from_int(rules::MELEE_CREEP_HP),
        },
    );
    world.step();
    let full = world.health.get(creep).expect("alive").hp;
    world.upgrades.insert(creep, Upgrades(3));
    world.step();
    let stats = world.stats.get(creep).expect("worked out this tick");
    assert_eq!(
        stats.max_hp,
        Fixed::from_int(rules::MELEE_CREEP_HP + 3 * rules::MELEE_UPGRADE_HP)
    );
    assert_eq!(stats.damage, rules::MELEE_CREEP_ATTACK_DAMAGE + 3);
    assert_eq!(
        world.health.get(creep).map(|h| h.hp),
        Some(full + Fixed::from_int(3 * rules::MELEE_UPGRADE_HP)),
        "the health gained is the maximum gained"
    );
}

#[test]
fn a_flag_carrier_takes_no_upgrades() {
    let mut world = World::new();
    let ranged = world.spawn();
    world.def.insert(ranged, Def(&RANGED_CREEP));
    world.upgrades.insert(ranged, Upgrades(3));
    let flag = world.spawn();
    world.def.insert(flag, Def(&FLAGBEARER_CREEP));
    world.upgrades.insert(flag, Upgrades(3));
    world.step();
    assert_eq!(
        world.stats.get(ranged).map(|s| s.max_hp),
        Some(Fixed::from_int(
            rules::RANGED_CREEP_HP + 3 * rules::RANGED_UPGRADE_HP
        ))
    );
    assert_eq!(
        world.stats.get(flag).map(|s| s.max_hp),
        Some(Fixed::from_int(rules::MELEE_CREEP_HP)),
        "upgrades pass a flag carrier by"
    );
}

#[test]
fn levels_raise_a_hero() {
    let mut world = World::new();
    let hero = world.spawn();
    world.def.insert(hero, Def(&HERO));
    world.level.insert(hero, Level(1));
    world.mana.insert(hero, Mana { mana: Fixed::ZERO });
    world.step();
    let first = world.stats.get(hero).expect("worked out this tick").max_hp;
    assert_eq!(first, Fixed::from_int(rules::HERO_HP), "level one is plain");
    world.level.insert(hero, Level(4));
    world.step();
    assert_eq!(
        world.stats.get(hero).map(|s| s.max_hp),
        Some(Fixed::from_int(
            rules::HERO_HP + 3 * rules::HERO_HP_PER_LEVEL
        )),
        "three levels past the first"
    );
}

#[test]
fn haste_shortens_the_wait_between_attacks_while_it_lasts() {
    let mut world = World::new();
    let creep = plain_creep(&mut world);
    world.statuses.insert(
        creep,
        Statuses(vec![Status {
            kind: StatusKind::Haste,
            ticks_left: 5,
            magnitude: 40,
        }]),
    );
    world.step();
    assert_eq!(
        world.stats.get(creep).map(|s| s.attack_interval),
        Some(rules::CREEP_ATTACK_INTERVAL * 60 / 100)
    );
    world.statuses.insert(creep, Statuses(Vec::new()));
    world.step();
    assert_eq!(
        world.stats.get(creep).map(|s| s.attack_interval),
        Some(rules::CREEP_ATTACK_INTERVAL),
        "what is worked out afresh forgets what has lifted"
    );
}

#[test]
fn mending_adds_to_what_a_kind_regenerates() {
    let mut world = World::new();
    let creep = plain_creep(&mut world);
    world.statuses.insert(
        creep,
        Statuses(vec![Status {
            kind: StatusKind::Mending,
            ticks_left: 5,
            magnitude: 25,
        }]),
    );
    world.step();
    assert_eq!(
        world.stats.get(creep).map(|s| s.hp_regen),
        Some(Fixed::from_ratio(25, 100)),
        "a creep mends nothing of its own, so this is all of it"
    );
}

#[test]
fn every_neutral_kind_has_numbers_of_its_own() {
    let dragon = NeutralKind::BlackDragon.def();
    assert_eq!(dragon.max_hp, 2000);
    assert_eq!(
        dragon.projectile_speed,
        Some(Fixed::from_int(1500).to_int())
    );
    assert!(dragon.ancient, "a dragon is an ancient creep");
    let kobold = NeutralKind::Kobold.def();
    assert_eq!(kobold.max_hp, 240);
    assert_eq!(kobold.projectile_speed, None, "a kobold swings");
    assert!(!kobold.ancient);
    assert_eq!(kobold.radius, rules::NEUTRAL_RADIUS);
    assert_eq!(kobold.vision, rules::NEUTRAL_VISION);
    assert_eq!(kobold.per_upgrade.hp, rules::NEUTRAL_UPGRADE_HP);
    assert_eq!(NEUTRALS.len(), 36);
}

#[test]
fn what_an_entity_carries_and_casts_keeps_its_slots() {
    let mut inventory = Inventory::empty(3);
    assert_eq!(inventory.held().count(), 0);
    inventory.slots[1] = Some(ItemStack {
        id: bota_proto::ItemId(7),
        charges: 2,
        cooldown: 0,
        bought_tick: 0,
        touched: false,
    });
    assert_eq!(inventory.held().count(), 1);
    assert_eq!(inventory.slots.len(), 3, "an empty slot keeps its number");

    let book = AbilityBook {
        slots: vec![
            AbilityState {
                id: bota_proto::AbilityId(1),
                level: 0,
                cooldown: 0,
            },
            AbilityState {
                id: bota_proto::AbilityId(2),
                level: 3,
                cooldown: 0,
            },
        ],
    };
    assert_eq!(
        book.learned().count(),
        1,
        "an unlearned ability is not held"
    );
    assert_eq!(book.slot(1).map(|a| a.level), Some(3));
}

#[test]
fn a_world_built_on_a_map_stands_its_buildings_full() {
    let map = crate::sim::map_of(bota_proto::MapId(1));
    let world = World::on_map(map);
    let towers = map.radiant_towers.len() + map.dire_towers.len();
    assert_eq!(
        world.entities.len(),
        towers + 4,
        "two fountains, two ancients"
    );
    let view = world.view_full();
    assert_eq!(view.units.len(), world.entities.len());
    let ancient = view
        .units
        .iter()
        .find(|u| u.kind == bota_proto::UnitKind::Ancient)
        .expect("an ancient stands");
    assert_eq!(ancient.hp, rules::ANCIENT_HP, "built and full");
    assert_eq!(ancient.max_hp, rules::ANCIENT_HP);
}

#[test]
fn both_sides_are_always_told_of_every_building() {
    let map = crate::sim::map_of(bota_proto::MapId(1));
    let world = World::on_map(map);
    let standing = map.radiant_towers.len() + map.dire_towers.len() + 4;
    let view = world.view(bota_proto::Team::Radiant);
    assert_eq!(
        view.units.len(),
        standing,
        "every building on the map is told, whosever it is"
    );
    let dire_fountain = map.fountains[1];
    assert!(
        view.units.iter().any(|u| u.pos == dire_fountain),
        "including the one across the map it has no eyes on"
    );
}

#[test]
fn a_unit_across_the_map_is_still_kept_from_a_side() {
    let map = crate::sim::map_of(bota_proto::MapId(1));
    let mut world = World::on_map(map);
    let far = world.spawn_unit(&MELEE_CREEP, bota_proto::Team::Dire, map.fountains[1]);
    world.settle();
    assert!(
        !world.can_see(bota_proto::Team::Radiant, far),
        "a creep is not a building"
    );
    let view = world.view(bota_proto::Team::Radiant);
    assert!(
        !view
            .units
            .iter()
            .any(|u| u.id == crate::engine::wire_id(far)),
        "and the fog keeps it back"
    );
}

#[test]
fn a_unit_still_standing_never_reads_as_empty() {
    let mut world = World::new();
    let creep = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Radiant,
        bota_proto::Vec2::ZERO,
    );
    world.settle();
    world.health.insert(
        creep,
        Health {
            hp: Fixed::from_ratio(1, 4),
        },
    );
    let view = world.view_full();
    let shown = view.units.first().expect("one unit").hp;
    assert_eq!(shown, 1, "a quarter of a point still shows as one");
}

#[test]
fn an_order_to_walk_moves_a_body_and_turns_it_first() {
    let mut world = World::new();
    let hero = world.spawn_hero(
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(1000, 1000),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    world.settle();
    world.orders.insert(
        hero,
        crate::engine::Orders {
            current: crate::engine::UnitOrder::Move {
                pos: bota_proto::Vec2::from_ints(0, 1000),
            },
            cooldown: 0,
        },
    );
    // Facing east and sent west: it turns before it takes a step.
    let start = world.transform.get(hero).expect("placed").pos;
    world.step();
    assert_eq!(
        world.transform.get(hero).map(|t| t.pos),
        Some(start),
        "the first tick is spent coming round"
    );
    for _ in 0..30 {
        world.step();
    }
    let now = world.transform.get(hero).expect("alive").pos;
    assert!(now.x < start.x, "it walks towards where it was sent");
    assert_eq!(now.y, start.y, "and holds the line it was sent along");
}

#[test]
fn a_side_sees_what_stands_inside_its_sight() {
    let mut world = World::new();
    let watcher = world.spawn_hero(
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(1000, 1000),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    let near = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(1200, 1000),
    );
    let far = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(9000, 9000),
    );
    world.settle();
    world.step();
    let seen_near = world.visibility.get(near).expect("worked out this tick");
    assert!(seen_near.by(bota_proto::Team::Radiant), "close enough");
    assert!(seen_near.by(bota_proto::Team::Dire), "its own side always");
    let seen_far = world.visibility.get(far).expect("worked out this tick");
    assert!(!seen_far.by(bota_proto::Team::Radiant), "out of sight");
    let view = world.view(bota_proto::Team::Radiant);
    let ids: Vec<_> = view.units.iter().map(|u| u.id).collect();
    assert!(ids.contains(&crate::engine::wire_id(watcher)));
    assert!(ids.contains(&crate::engine::wire_id(near)));
    assert!(
        !ids.contains(&crate::engine::wire_id(far)),
        "fog holds it back"
    );
}

#[test]
fn a_tower_takes_the_nearest_enemy_and_brings_it_down() {
    let mut world = World::new();
    let tower = world.spawn_unit(
        crate::engine::tower_def(1),
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(1000, 1000),
    );
    let near = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(1200, 1000),
    );
    let far = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(1600, 1000),
    );
    world.settle();
    world.step();
    assert_eq!(
        world.target_of(tower),
        Some(near),
        "the nearer one is taken"
    );
    let mut ticks = 0;
    while world.alive(near) && ticks < 900 {
        world.step();
        ticks += 1;
    }
    assert!(!world.alive(near), "a tower brings a creep down");
    assert!(world.alive(far), "the other one was never in reach");
    assert!(!world.entities.contains(near), "what falls is cleared away");
}

#[test]
fn a_fallen_ancient_ends_the_match() {
    let mut world = World::new();
    let ancient = world.spawn_unit(
        &crate::engine::ANCIENT,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(1000, 1000),
    );
    world.settle();
    // An Ancient shrugs damage off while it stands invulnerable, so this one
    // is brought down directly.
    world.health.insert(ancient, Health { hp: Fixed::ZERO });
    let mut events = Vec::new();
    world.bury(vec![(ancient, None)], &mut events);
    assert_eq!(world.winner, Some(bota_proto::Team::Radiant));
    assert!(
        events
            .iter()
            .any(|e| matches!(e.kind, bota_proto::EventKind::StructureDestroyed { .. }))
    );
}

#[test]
fn a_missile_carries_the_hit_rather_than_landing_it_at_once() {
    let mut world = World::new();
    let archer = world.spawn_unit(
        &RANGED_CREEP,
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(1000, 1000),
    );
    let mark = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(1400, 1000),
    );
    world.settle();
    let full = world.health.get(mark).expect("standing").hp;
    for _ in 0..20 {
        world.step();
    }
    assert!(
        world.projectile.get(archer).is_none(),
        "the archer is not its own missile"
    );
    let mut flying = 0;
    for entity in world.entities.iter() {
        if world.projectile.get(entity).is_some() {
            flying += 1;
        }
    }
    assert!(flying > 0 || world.health.get(mark).expect("standing").hp < full);
}

#[test]
fn a_wave_arrives_on_the_clock_and_walks_its_lane() {
    let map = crate::sim::map_of(bota_proto::MapId(1));
    let mut world = World::on_map(map);
    while world.tick < rules::FIRST_WAVE_TICK {
        world.step();
    }
    let wave: Vec<_> = world
        .entities
        .iter()
        .filter(|e| world.march.get(*e).is_some())
        .collect();
    let plan = crate::sim::wave_plan(1);
    let per_side = (plan.melee + plan.ranged + plan.siege) as usize;
    assert_eq!(
        wave.len(),
        per_side * 2 * usize::from(map.lanes),
        "one wave a lane a side"
    );
    let radiant: Vec<_> = wave
        .iter()
        .copied()
        .filter(|e| world.team.get(*e) == Some(&bota_proto::Team::Radiant))
        .collect();
    let start = world.transform.get(radiant[0]).expect("just placed").pos.x;
    for _ in 0..90 {
        world.step();
    }
    let now = world.transform.get(radiant[0]).expect("alive").pos.x;
    assert!(
        now > start,
        "a Radiant creep walks up its lane: {now:?} from {start:?}"
    );
}

#[test]
fn a_wave_carries_one_flag_from_the_fifth_wave_on() {
    let map = crate::sim::map_of(bota_proto::MapId(1));
    let mut world = World::on_map(map);
    let fifth = rules::FIRST_WAVE_TICK + 4 * rules::WAVE_PERIOD_TICKS;
    while world.tick < fifth {
        world.step();
    }
    let flags = world
        .entities
        .iter()
        .filter(|e| world.kind.get(*e) == Some(&bota_proto::UnitKind::CreepFlagbearer))
        .count();
    assert_eq!(flags, 2 * usize::from(map.lanes), "one a lane a side");
}

#[test]
fn a_unit_is_taken_before_a_building_and_a_siege_creep_takes_the_building_first() {
    let mut world = World::new();
    let at = bota_proto::Vec2::from_ints(1000, 1000);
    let creep = world.spawn_unit(&MELEE_CREEP, bota_proto::Team::Radiant, at);
    let siege = world.spawn_unit(&crate::engine::SIEGE_CREEP, bota_proto::Team::Radiant, at);
    let tower = world.spawn_unit(
        crate::engine::tower_def(1),
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(1100, 1000),
    );
    let enemy = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(1300, 1000),
    );
    world.settle();
    let reach = world.stats.get(creep).expect("settled").acquisition;
    assert_eq!(
        world.acquire(creep, reach, crate::engine::PriorityOrder::Normal),
        Some(enemy),
        "a unit outranks a building however much nearer the building stands"
    );
    let siege_reach = world.stats.get(siege).expect("settled").acquisition;
    assert_eq!(
        world.acquire(siege, siege_reach, crate::engine::PriorityOrder::SiegeFirst),
        Some(tower),
        "a siege creep goes for the building"
    );
    assert_eq!(
        world.priority_of(siege),
        crate::engine::PriorityOrder::SiegeFirst
    );
    assert_eq!(
        world.priority_of(creep),
        crate::engine::PriorityOrder::Normal
    );
}

#[test]
fn a_building_never_shoots_the_jungle_and_a_creep_only_at_a_pull_camp() {
    let mut world = World::new();
    let tower = world.spawn_unit(
        crate::engine::tower_def(1),
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(1000, 1000),
    );
    let creep = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(1000, 1000),
    );
    let hero = world.spawn_hero(
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(1000, 1000),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    let beast = world.spawn_unit(
        crate::engine::NeutralKind::Kobold.def(),
        bota_proto::Team::Neutral,
        bota_proto::Vec2::from_ints(1100, 1000),
    );
    world.settle();
    assert!(!world.hostile(tower, beast), "a tower leaves the jungle be");
    assert!(world.hostile(hero, beast), "a hero may hit it");
    assert!(
        !world.hostile(creep, beast),
        "a lane creep leaves a camp it cannot be pulled to"
    );
    let pull = crate::sim::CAMPS
        .iter()
        .find(|c| c.pullable)
        .expect("the map marks pull camps");
    world.camp_home.insert(
        beast,
        crate::engine::CampHome {
            camp: 0,
            home: pull.pos,
        },
    );
    assert!(
        world.hostile(creep, beast),
        "at a pull camp it will fight after all"
    );
}

#[test]
fn a_body_does_not_walk_through_a_building() {
    let map = crate::sim::map_of(bota_proto::MapId(1));
    let mut world = World::on_map(map);
    let (_, _, tower) = map.radiant_towers[0];
    let hero = world.spawn_hero(
        bota_proto::Team::Radiant,
        tower - bota_proto::Vec2::from_ints(400, 0),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    world.settle();
    world.orders.insert(
        hero,
        crate::engine::Orders {
            current: crate::engine::UnitOrder::Move {
                pos: tower + bota_proto::Vec2::from_ints(400, 0),
            },
            cooldown: 0,
        },
    );
    let mut nearest = i64::MAX;
    for _ in 0..300 {
        world.step();
        let at = world.transform.get(hero).expect("alive").pos;
        nearest = nearest.min(crate::sim::isqrt64(at.distance_squared(tower)));
    }
    let hull = world.hull.get(hero).expect("has one").radius;
    assert!(
        nearest > i64::from(hull.raw),
        "it never stood inside the tower"
    );
    let end = world.transform.get(hero).expect("alive").pos;
    assert!(end.x > tower.x, "and it got past all the same: {end:?}");
}

#[test]
fn two_bodies_on_one_spot_are_eased_apart() {
    let mut world = World::new();
    let at = bota_proto::Vec2::from_ints(5000, 5000);
    let one = world.spawn_unit(&MELEE_CREEP, bota_proto::Team::Radiant, at);
    let other = world.spawn_unit(&MELEE_CREEP, bota_proto::Team::Radiant, at);
    world.settle();
    for _ in 0..30 {
        world.step();
    }
    let (a, b) = (
        world.transform.get(one).expect("alive").pos,
        world.transform.get(other).expect("alive").pos,
    );
    let hulls = world.hull.get(one).expect("has one").radius
        + world.hull.get(other).expect("has one").radius;
    // Easing apart stops at the moment they stop overlapping, which leaves
    // them touching exactly.
    let apart = crate::sim::isqrt64(a.distance_squared(b));
    assert!(
        apart >= i64::from(hulls.raw),
        "still inside one another: {a:?} {b:?}"
    );
}

#[test]
fn camps_fill_on_the_minute_and_stay_full() {
    let map = crate::sim::map_of(bota_proto::MapId(1));
    let mut world = World::on_map(map);
    while world.tick < rules::FIRST_NEUTRAL_TICK {
        world.step();
    }
    let filled = world
        .entities
        .iter()
        .filter(|e| world.camp_home.get(*e).is_some())
        .count();
    assert!(filled > 0, "the jungle put something out");
    let before = filled;
    for _ in 0..rules::NEUTRAL_SPAWN_PERIOD_TICKS {
        world.step();
    }
    let now = world
        .entities
        .iter()
        .filter(|e| world.camp_home.get(*e).is_some())
        .count();
    assert_eq!(now, before, "a full camp puts out nothing more");
}

#[test]
fn a_neutral_led_too_far_gives_up_and_walks_home() {
    let map = crate::sim::map_of(bota_proto::MapId(1));
    let mut world = World::on_map(map);
    while world.tick < rules::FIRST_NEUTRAL_TICK {
        world.step();
    }
    let beast = world
        .entities
        .iter()
        .find(|e| world.camp_home.get(*e).is_some())
        .expect("the jungle is full");
    let home = world.camp_home.get(beast).expect("it has one").home;
    // Carried well past its guard distance and held there.
    let away = home + bota_proto::Vec2::from_ints(1200, 0);
    if let Some(transform) = world.transform.get_mut(beast) {
        transform.pos = away;
    }
    for _ in 0..rules::NEUTRAL_AGGRO_WINDOW + 2 {
        world.step();
        if let Some(transform) = world.transform.get_mut(beast) {
            transform.pos = away;
        }
    }
    assert!(
        world.neutral_ai.get(beast).is_some_and(|ai| ai.going_home),
        "its patience ran out"
    );
    assert!(
        world.target.get(beast).is_none(),
        "and it takes nothing on while it walks back"
    );
}

#[test]
fn a_kill_pays_the_one_who_struck_last_and_feeds_the_side() {
    let mut world = World::new();
    let hero = world.spawn_hero(
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(5000, 5000),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    world.seats.push(crate::engine::Seat::new(
        bota_proto::SlotId(0),
        bota_proto::Team::Radiant,
        bota_proto::HeroId(0),
        0,
        rules::STASH_SLOTS,
    ));
    world.seats[0].unit = Some(hero);
    let prey = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(5050, 5000),
    );
    world.settle();
    world.health.insert(prey, Health { hp: Fixed::ZERO });
    let mut events = Vec::new();
    world.bury(vec![(prey, Some(hero))], &mut events);
    assert_eq!(world.seats[0].gold, rules::MELEE_CREEP_BOUNTY);
    assert_eq!(world.seats[0].last_hits, 1);
    assert_eq!(world.seats[0].xp, rules::MELEE_CREEP_XP);
}

#[test]
fn bringing_down_your_own_is_a_deny_and_pays_the_other_side_nothing() {
    let mut world = World::new();
    let hero = world.spawn_hero(
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(5000, 5000),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    world.seats.push(crate::engine::Seat::new(
        bota_proto::SlotId(0),
        bota_proto::Team::Radiant,
        bota_proto::HeroId(0),
        0,
        rules::STASH_SLOTS,
    ));
    world.seats[0].unit = Some(hero);
    let own = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(5050, 5000),
    );
    world.settle();
    let mut events = Vec::new();
    world.bury(vec![(own, Some(hero))], &mut events);
    assert_eq!(world.seats[0].denies, 1);
    assert_eq!(world.seats[0].gold, 0, "a deny pays no gold");
    assert_eq!(world.seats[0].xp, 0, "and no experience");
}

#[test]
fn a_fallen_hero_comes_back_at_its_fountain() {
    let map = crate::sim::map_of(bota_proto::MapId(1));
    let mut world = World::on_map(map);
    let hero = world.spawn_hero(
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(9000, 9216),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    world.seats.push(crate::engine::Seat::new(
        bota_proto::SlotId(0),
        bota_proto::Team::Radiant,
        bota_proto::HeroId(0),
        0,
        rules::STASH_SLOTS,
    ));
    world.seats[0].unit = Some(hero);
    world.settle();
    let mut events = Vec::new();
    world.bury(vec![(hero, None)], &mut events);
    assert!(world.seats[0].unit.is_none(), "its body is gone");
    assert_eq!(world.seats[0].deaths, 1);
    let wait = world.seats[0].respawn_left;
    assert!(wait > 0);
    for _ in 0..=wait {
        world.step();
    }
    let back = world.seats[0].unit.expect("it came back");
    let at = world.transform.get(back).expect("standing").pos;
    // It comes back at the fountain and is eased out of its hull at once.
    let fountain = crate::sim::fountain_pos(map, bota_proto::Team::Radiant);
    assert!(
        at.within(fountain, rules::units(rules::FOUNTAIN_RADIUS + 100)),
        "it came back somewhere else: {at:?}"
    );
    let full = world.stats.get(back).expect("settled").max_hp;
    assert_eq!(world.health.get(back).map(|h| h.hp), Some(full), "and full");
}

#[test]
fn what_a_hero_carries_shows_up_in_its_stats() {
    let mut world = World::new();
    let hero = world.spawn_hero(
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(5000, 5000),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    world.step();
    let bare = world.stats.get(hero).expect("settled").damage;
    // The first item in the table that carries damage rather than charges.
    let (id, def) = rules::ITEMS
        .iter()
        .enumerate()
        .find(|(_, d)| d.charges == 0 && d.damage > 0)
        .expect("some item adds damage");
    if let Some(bag) = world.inventory.get_mut(hero) {
        bag.slots[0] = Some(crate::engine::ItemStack {
            id: bota_proto::ItemId(id as u16),
            charges: 0,
            cooldown: 0,
            bought_tick: 0,
            touched: false,
        });
    }
    world.step();
    assert_eq!(
        world.stats.get(hero).map(|s| s.damage),
        Some(bare + def.damage),
        "the damage it carries is added"
    );
    if let Some(bag) = world.inventory.get_mut(hero) {
        bag.slots[0] = None;
    }
    world.step();
    assert_eq!(
        world.stats.get(hero).map(|s| s.damage),
        Some(bare),
        "and dropping it takes the damage away, with nothing to unapply"
    );
}

#[test]
fn a_salve_puts_mending_on_whoever_drinks_it_and_runs_out() {
    let mut world = World::new();
    let hero = world.spawn_hero(
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(5000, 5000),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    world.step();
    if let Some(bag) = world.inventory.get_mut(hero) {
        bag.slots[0] = Some(crate::engine::ItemStack {
            id: bota_proto::ItemId(rules::ITEM_SALVE),
            charges: 1,
            cooldown: 0,
            bought_tick: 0,
            touched: false,
        });
    }
    assert!(world.use_item(hero, 0), "it drinks");
    assert!(
        world.inventory.get(hero).expect("has a bag").slots[0].is_none(),
        "the last charge takes the stack with it"
    );
    world.step();
    let plain = crate::engine::HERO.hp_regen;
    assert!(
        world.stats.get(hero).expect("settled").hp_regen > plain,
        "it mends faster while the salve holds"
    );
    for _ in 0..rules::REGEN_BUFF_TICKS + 1 {
        world.step();
    }
    assert_eq!(
        world.stats.get(hero).map(|s| s.hp_regen),
        Some(plain),
        "and back to its own once it runs out"
    );
}

#[test]
fn a_side_is_told_only_of_what_it_could_see() {
    let map = crate::sim::map_of(bota_proto::MapId(1));
    let mut world = World::on_map(map);
    let far = bota_proto::Vec2::from_ints(9600, 12000);
    assert!(
        !world.can_see_point(bota_proto::Team::Radiant, far),
        "nothing of that side stands anywhere near"
    );
    assert_eq!(
        world.who_may_know(far, bota_proto::Team::Dire),
        crate::sim::EventVisibility::OneTeam(bota_proto::Team::Dire),
        "only the side party to it is told"
    );
    let watcher = world.spawn_hero(
        bota_proto::Team::Radiant,
        far,
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    world.settle();
    assert!(world.can_see_point(bota_proto::Team::Radiant, far));
    assert_eq!(
        world.who_may_know(far, bota_proto::Team::Dire),
        crate::sim::EventVisibility::Everyone
    );
    let _ = watcher;
}

#[test]
fn an_order_at_something_a_side_cannot_see_is_refused() {
    let map = crate::sim::map_of(bota_proto::MapId(1));
    let mut world = World::on_map(map);
    let hero = world.spawn_hero(
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(6800, 9216),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    world.seats.push(crate::engine::Seat::new(
        bota_proto::SlotId(0),
        bota_proto::Team::Radiant,
        bota_proto::HeroId(0),
        0,
        rules::STASH_SLOTS,
    ));
    world.seats[0].unit = Some(hero);
    let hidden = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(13200, 9216),
    );
    world.settle();
    crate::engine::visibility_system(crate::engine::SightCx {
        entities: &world.entities,
        transform: &world.transform,
        team: &world.team,
        kind: &world.kind,
        stats: &world.stats,
        ground: &world.ground,
        sight_block: &world.sight_block,
        visibility: &mut world.visibility,
    });
    let order = bota_proto::Order::AttackUnit {
        target: crate::engine::wire_id(hidden),
    };
    assert_eq!(
        world.validate_order(bota_proto::SlotId(0), &order),
        Err(bota_proto::RejectReason::UnknownTarget),
        "it is nowhere near and cannot be ordered at"
    );
    let near = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(6900, 9216),
    );
    world.settle();
    let order = bota_proto::Order::AttackUnit {
        target: crate::engine::wire_id(near),
    };
    assert_eq!(world.validate_order(bota_proto::SlotId(0), &order), Ok(()));
}

#[test]
fn a_seat_with_no_body_standing_may_order_nothing() {
    let mut world = World::new();
    world.seats.push(crate::engine::Seat::new(
        bota_proto::SlotId(0),
        bota_proto::Team::Radiant,
        bota_proto::HeroId(0),
        0,
        rules::STASH_SLOTS,
    ));
    let order = bota_proto::Order::Move {
        pos: bota_proto::Vec2::ZERO,
    };
    assert_eq!(
        world.validate_order(bota_proto::SlotId(0), &order),
        Err(bota_proto::RejectReason::HeroDead)
    );
}

/// A match config that always names the same numbers.
fn config() -> crate::sim::MatchConfig {
    crate::sim::MatchConfig {
        match_id: 7,
        master_key: [3; 32],
        picks: vec![bota_proto::Pick {
            slot: bota_proto::SlotId(0),
            team: bota_proto::Team::Radiant,
            hero: bota_proto::HeroId(0),
        }],
        map: bota_proto::MapId(1),
        tick_rate: 30,
        mode: bota_proto::TickMode::Lockstep,
        ack_timeout_ticks: 30,
    }
}

#[test]
fn two_runs_of_one_script_agree_at_every_checkpoint() {
    let mut first = World::for_match(&config(), config().rng());
    let mut second = World::for_match(&config(), config().rng());
    assert_eq!(first.hash(), second.hash(), "they start the same");
    for tick in 1..=600u32 {
        first.step();
        second.step();
        if tick % 60 == 0 {
            assert_eq!(first.hash(), second.hash(), "they parted at tick {tick}");
        }
    }
}

#[test]
fn the_fingerprint_moves_when_the_world_does() {
    let mut world = World::for_match(&config(), config().rng());
    let before = world.hash();
    world.step();
    assert_ne!(before, world.hash(), "a tick is a change");
    let creep = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(9000, 9216),
    );
    world.settle();
    let with = world.hash();
    if let Some(health) = world.health.get_mut(creep) {
        health.hp -= Fixed::ONE;
    }
    assert_ne!(with, world.hash(), "so is a point of health");
}

/// A lane creep on clear ground with a mind of its own.
fn thinking_creep(world: &mut World, at: bota_proto::Vec2) -> Entity {
    let creep = world.spawn_unit(&MELEE_CREEP, bota_proto::Team::Radiant, at);
    world.lane_ai.insert(
        creep,
        crate::engine::LaneAi {
            anchor: None,
            chase_left: 0,
            provoked: 0,
            last_seen: None,
        },
    );
    creep
}

#[test]
fn a_creep_gives_up_a_chase_it_cannot_finish() {
    let mut world = World::new();
    let creep = thinking_creep(&mut world, bota_proto::Vec2::from_ints(5000, 5000));
    let prey = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(5100, 5000),
    );
    world.settle();
    world.step();
    assert_eq!(world.target_of(creep), Some(prey), "it takes it");
    // Carried out of everything it can see, and the creep held where it was.
    let away = bota_proto::Vec2::from_ints(5900, 5000);
    for _ in 0..rules::CREEP_CHASE_TICKS + 2 {
        if let Some(at) = world.transform.get_mut(prey) {
            at.pos = away;
        }
        if let Some(at) = world.transform.get_mut(creep) {
            at.pos = bota_proto::Vec2::from_ints(5000, 5000);
        }
        world.step();
    }
    assert_eq!(
        world.target_of(creep),
        None,
        "the chase ran out and it let go"
    );
}

#[test]
fn a_creep_walks_back_to_where_it_left_its_route() {
    let mut world = World::new();
    let start = bota_proto::Vec2::from_ints(5000, 5000);
    let creep = thinking_creep(&mut world, start);
    world.settle();
    if let Some(ai) = world.lane_ai.get_mut(creep) {
        ai.anchor = Some(start);
    }
    if let Some(at) = world.transform.get_mut(creep) {
        at.pos = bota_proto::Vec2::from_ints(5600, 5000);
    }
    world.step();
    let order = world.orders.get(creep).map(|o| o.current);
    assert_eq!(
        order,
        Some(crate::engine::UnitOrder::AttackMove { pos: start }),
        "it is sent back to where it left"
    );
}

#[test]
fn an_attack_order_at_an_ally_never_hands_the_creep_the_one_who_gave_it() {
    let mut world = World::new();
    let creep = thinking_creep(&mut world, bota_proto::Vec2::from_ints(5000, 5000));
    let hero = world.spawn_hero(
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(5040, 5000),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    let other = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(5300, 5000),
    );
    world.settle();
    world.provoke(creep, hero, true);
    assert_eq!(
        world.target_of(creep),
        Some(other),
        "the nearer hero is put last, so the creep takes the creep"
    );
    world.provoke(creep, hero, false);
    assert_eq!(
        world.target_of(creep),
        Some(hero),
        "an attack at an enemy hands it over outright"
    );
    assert_eq!(
        world.lane_ai.get(creep).map(|ai| ai.provoked),
        Some(rules::ORDER_AGGRO_HOLD_TICKS),
        "and holds it there for a while"
    );
}

/// A hero that has learned one ability to its first level, with mana to spend.
fn caster(world: &mut World, at: bota_proto::Vec2, slot: usize) -> Entity {
    let hero = world.spawn_hero(
        bota_proto::Team::Radiant,
        at,
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    world.settle();
    world.fill_pools(hero);
    let mut events = Vec::new();
    world.learn(hero, slot, &mut events);
    hero
}

#[test]
fn frenzy_puts_haste_on_its_caster_and_spends_the_mana() {
    let mut world = World::new();
    let hero = caster(&mut world, bota_proto::Vec2::from_ints(5000, 5000), 1);
    let full = world.mana.get(hero).expect("has mana").mana;
    let plain = world.stats.get(hero).expect("settled").attack_interval;
    world.order_cast(
        hero,
        crate::engine::PendingCast {
            slot: bota_proto::AbilitySlot(1),
            target: bota_proto::OrderTarget::None,
        },
    );
    world.step();
    assert!(
        world.mana.get(hero).expect("has mana").mana < full,
        "a cast costs mana"
    );
    // Stats are worked out before casts run, so what a cast puts on shows
    // from the tick after.
    world.step();
    assert!(
        world.stats.get(hero).expect("settled").attack_interval < plain,
        "and it swings faster while the haste holds"
    );
    assert!(
        world.abilities.get(hero).expect("has a book").slots[1].cooldown > 0,
        "and waits before casting again"
    );
}

#[test]
fn a_cast_with_no_mana_behind_it_does_nothing() {
    let mut world = World::new();
    let hero = caster(&mut world, bota_proto::Vec2::from_ints(5000, 5000), 1);
    if let Some(mana) = world.mana.get_mut(hero) {
        mana.mana = Fixed::ZERO;
    }
    world.order_cast(
        hero,
        crate::engine::PendingCast {
            slot: bota_proto::AbilitySlot(1),
            target: bota_proto::OrderTarget::None,
        },
    );
    world.step();
    assert_eq!(
        world.abilities.get(hero).expect("has a book").slots[1].cooldown,
        0,
        "nothing was spent and nothing began"
    );
}

#[test]
fn a_multishot_strikes_everything_around_and_leaves_allies_be() {
    let mut world = World::new();
    let at = bota_proto::Vec2::from_ints(5000, 5000);
    let hero = caster(&mut world, at, 3);
    let near = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(5200, 5000),
    );
    let ally = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(5200, 5100),
    );
    let far = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(7000, 5000),
    );
    world.settle();
    world.fill_pools(near);
    world.fill_pools(ally);
    world.fill_pools(far);
    let (was_near, was_ally, was_far) = (
        world.health.get(near).expect("standing").hp,
        world.health.get(ally).expect("standing").hp,
        world.health.get(far).expect("standing").hp,
    );
    world.order_cast(
        hero,
        crate::engine::PendingCast {
            slot: bota_proto::AbilitySlot(3),
            target: bota_proto::OrderTarget::None,
        },
    );
    world.step();
    assert!(
        world.health.get(near).expect("standing").hp < was_near,
        "the one in the ring is struck"
    );
    assert_eq!(
        world.health.get(ally).expect("standing").hp,
        was_ally,
        "its own side is left be"
    );
    assert_eq!(
        world.health.get(far).expect("standing").hp,
        was_far,
        "and the one outside the ring is untouched"
    );
}

#[test]
fn buying_needs_the_shop() {
    let map = crate::sim::map_of(bota_proto::MapId(1));
    let mut world = World::on_map(map);
    let away = bota_proto::Vec2::from_ints(9600, 9216);
    let hero = world.spawn_hero(
        bota_proto::Team::Radiant,
        away,
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    let mut seat = crate::engine::Seat::new(
        bota_proto::SlotId(0),
        bota_proto::Team::Radiant,
        bota_proto::HeroId(0),
        rules::STARTING_GOLD,
        rules::STASH_SLOTS,
    );
    seat.unit = Some(hero);
    world.seats.push(seat);
    world.settle();
    let mut events = Vec::new();
    let salve = bota_proto::ItemId(rules::ITEM_SALVE);
    assert!(
        !world.buy(bota_proto::SlotId(0), salve, &mut events),
        "out in the lane there is nothing to buy from"
    );
    if let Some(at) = world.transform.get_mut(hero) {
        at.pos = crate::sim::fountain_pos(map, bota_proto::Team::Radiant);
    }
    assert!(
        world.buy(bota_proto::SlotId(0), salve, &mut events),
        "at its own shop it may"
    );
    assert!(world.seats[0].gold < rules::STARTING_GOLD, "and it paid");
}

#[test]
fn a_ranged_attack_puts_a_missile_where_a_side_can_see_it() {
    let mut world = World::new();
    let archer = world.spawn_hero(
        bota_proto::Team::Radiant,
        bota_proto::Vec2::from_ints(5000, 5000),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    let mark = world.spawn_unit(
        &MELEE_CREEP,
        bota_proto::Team::Dire,
        bota_proto::Vec2::from_ints(5300, 5000),
    );
    world.settle();
    world.fill_pools(archer);
    world.fill_pools(mark);
    let mut flew = false;
    for _ in 0..40 {
        world.step();
        if !world.view_full().projectiles.is_empty() {
            flew = true;
            break;
        }
    }
    assert!(
        flew,
        "a ranged hero throws something the client is told about"
    );
    let view = world.view(bota_proto::Team::Radiant);
    assert!(
        !view.projectiles.is_empty(),
        "and its own side is told of it"
    );
}

#[test]
fn a_creep_is_sent_one_way_at_a_time() {
    let map = crate::sim::map_of(bota_proto::MapId(1));
    let mut world = World::on_map(map);
    while world.tick < rules::FIRST_WAVE_TICK {
        world.step();
    }
    let creep = world
        .entities
        .iter()
        .find(|e| {
            world.march.get(*e).is_some() && world.team.get(*e) == Some(&bota_proto::Team::Radiant)
        })
        .expect("a wave came out");
    // Left the route: the mark it was given must not change from tick to tick
    // while nothing about it changes.
    if let Some(ai) = world.lane_ai.get_mut(creep) {
        ai.anchor = Some(bota_proto::Vec2::from_ints(6800, 9600));
    }
    world.step();
    let first = world.orders.get(creep).map(|o| o.current);
    world.step();
    let second = world.orders.get(creep).map(|o| o.current);
    assert_eq!(first, second, "it is not pulled two ways in one breath");
    let x = world.transform.get(creep).expect("alive").pos.x.to_int();
    for _ in 0..60 {
        world.step();
    }
    let later = world.transform.get(creep).expect("alive").pos.x.to_int();
    assert!(
        (later - x).abs() > 40,
        "and it actually goes somewhere: {x} then {later}"
    );
}

#[test]
fn a_visibility_row_belongs_to_whatever_stands_on_a_side() {
    let mut world = World::new();
    let entity = world.spawn();
    assert_eq!(
        world.visibility.get(entity),
        None,
        "what stands on no side is not something sides see"
    );
    world.set_team(entity, Team::Radiant);
    assert!(
        world
            .visibility
            .get(entity)
            .is_some_and(|s| s.by(Team::Radiant)),
        "taking a side makes the row, and that side has it at once"
    );
    world.step();
    assert!(
        world
            .visibility
            .get(entity)
            .is_some_and(|s| s.by(Team::Radiant)),
        "its own side has it from the first tick"
    );
    assert!(world.despawn(entity));
    assert_eq!(world.visibility.get(entity), None, "and given up with it");
    let next = world.spawn();
    assert_eq!(next.index(), entity.index(), "the slot came back round");
    assert_eq!(
        world.visibility.get(next),
        None,
        "and the new tenant inherits nothing"
    );
}

#[test]
fn a_missile_is_seen_from_the_tick_it_is_thrown() {
    let mut world = World::new();
    let archer = world.spawn_unit(
        &RANGED_CREEP,
        Team::Radiant,
        bota_proto::Vec2::from_ints(5000, 5000),
    );
    let watcher = world.spawn_unit(
        &MELEE_CREEP,
        Team::Dire,
        bota_proto::Vec2::from_ints(5300, 5000),
    );
    world.settle();
    let mut thrown = None;
    for _ in 0..40 {
        world.step();
        thrown = world
            .entities
            .iter()
            .find(|e| world.projectile.get(*e).is_some());
        if thrown.is_some() {
            break;
        }
    }
    let missile = thrown.expect("the archer threw something");
    assert!(
        world
            .visibility
            .get(missile)
            .is_some_and(|s| s.by(Team::Radiant)),
        "its own side has it"
    );
    let _ = (archer, watcher);
}

/// Runs one tick of the attack cycle and nothing else.
fn swing_once(world: &mut World) {
    crate::engine::attacking_system(crate::engine::AttackCx {
        entities: &mut world.entities,
        transform: &mut world.transform,
        hull: &world.hull,
        team: &mut world.team,
        health: &world.health,
        stats: &world.stats,
        visibility: &mut world.visibility,
        target: &world.target,
        attacking: &mut world.attacking,
        hit: &mut world.hit,
        projectile: &mut world.projectile,
    });
}

/// An attacker and its mark, standing where they are put and nothing else.
fn duel(gap: i32) -> (World, Entity, Entity) {
    let mut world = World::new();
    let at = bota_proto::Vec2::from_ints(5000, 5000);
    let attacker = world.spawn_unit(&MELEE_CREEP, Team::Radiant, at);
    let mark = world.spawn_unit(
        &MELEE_CREEP,
        Team::Dire,
        bota_proto::Vec2::from_ints(5000 + gap, 5000),
    );
    world.settle();
    (world, attacker, mark)
}

#[test]
fn a_swing_waits_on_the_angle_it_is_looking_at() {
    let (mut world, attacker, mark) = duel(100);
    world.set_target(attacker, mark);
    // Turned right away from it: in reach, seen, but not looked at.
    if let Some(at) = world.transform.get_mut(attacker) {
        at.facing = bota_proto::Angle { brads: 32768 };
    }
    swing_once(&mut world);
    assert_eq!(
        world.attacking.get(attacker).and_then(|a| a.windup),
        None,
        "nothing begins while it is looking the other way"
    );
    // Looking straight at it.
    if let Some(at) = world.transform.get_mut(attacker) {
        at.facing = bota_proto::Angle { brads: 0 };
    }
    swing_once(&mut world);
    assert!(
        world
            .attacking
            .get(attacker)
            .and_then(|a| a.windup)
            .is_some(),
        "and begins once it is"
    );
}

#[test]
fn a_swing_waits_on_being_able_to_see_at_all() {
    let (mut world, attacker, mark) = duel(100);
    world.set_target(attacker, mark);
    if let Some(seen) = world.visibility.get_mut(mark) {
        seen.clear();
    }
    swing_once(&mut world);
    assert_eq!(
        world.attacking.get(attacker).and_then(|a| a.windup),
        None,
        "what a side has no eyes on it does not swing at"
    );
}

#[test]
fn a_swing_waits_on_reach() {
    let (mut world, attacker, mark) = duel(600);
    world.set_target(attacker, mark);
    swing_once(&mut world);
    assert_eq!(
        world.attacking.get(attacker).and_then(|a| a.windup),
        None,
        "too far to touch"
    );
}

#[test]
fn a_swing_lands_on_whoever_it_began_against() {
    let (mut world, attacker, mark) = duel(100);
    let other = world.spawn_unit(
        &MELEE_CREEP,
        Team::Dire,
        bota_proto::Vec2::from_ints(5100, 5000),
    );
    world.settle();
    world.set_target(attacker, mark);
    world.step();
    assert!(
        world
            .attacking
            .get(attacker)
            .and_then(|a| a.windup)
            .is_some(),
        "the swing began"
    );
    // Set on somebody else halfway through.
    world.set_target(attacker, other);
    let was = world.health.get(mark).expect("standing").hp;
    for _ in 0..rules::MELEE_CREEP_ATTACK_POINT + 1 {
        world.step();
    }
    assert!(
        world.health.get(mark).expect("standing").hp < was,
        "and landed on the one it began against"
    );
}

#[test]
fn a_hero_told_to_attack_comes_round_and_closes() {
    let mut world = World::new();
    let hero = world.spawn_hero(
        Team::Radiant,
        bota_proto::Vec2::from_ints(5000, 5000),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    let mark = world.spawn_unit(
        &MELEE_CREEP,
        Team::Dire,
        bota_proto::Vec2::from_ints(4000, 5000),
    );
    world.settle();
    // Ordered at it, the way a player does.
    world.orders.insert(
        hero,
        crate::engine::Orders {
            current: crate::engine::UnitOrder::Attack {
                target: mark,
                last_seen: bota_proto::Vec2::from_ints(4000, 5000),
            },
            cooldown: 0,
        },
    );
    let start = world.transform.get(hero).expect("standing").pos;
    for _ in 0..60 {
        world.step();
    }
    let now = world.transform.get(hero).expect("standing").pos;
    assert!(now.x < start.x, "it walked at what it was set on: {now:?}");
}

#[test]
fn a_swing_that_began_still_connects_a_little_past_reach() {
    let (mut world, attacker, mark) = duel(100);
    world.set_target(attacker, mark);
    world.step();
    assert!(
        world
            .attacking
            .get(attacker)
            .and_then(|a| a.windup)
            .is_some(),
        "the swing began in reach"
    );
    // Backs off by less than the leeway while the swing is under way.
    let just_out = rules::MELEE_CREEP_ATTACK_RANGE + 60;
    let was = world.health.get(mark).expect("standing").hp;
    for _ in 0..rules::MELEE_CREEP_ATTACK_POINT + 1 {
        if let Some(at) = world.transform.get_mut(mark) {
            at.pos = bota_proto::Vec2::from_ints(5000 + just_out, 5000);
        }
        world.step();
    }
    assert!(
        world.health.get(mark).expect("standing").hp < was,
        "a step past reach does not shake it off"
    );
}

#[test]
fn a_swing_is_given_up_when_the_target_gets_away() {
    let (mut world, attacker, mark) = duel(100);
    world.set_target(attacker, mark);
    world.step();
    assert!(
        world
            .attacking
            .get(attacker)
            .and_then(|a| a.windup)
            .is_some(),
        "the swing began in reach"
    );
    let far = rules::MELEE_CREEP_ATTACK_RANGE + rules::ATTACK_RANGE_LEEWAY + 200;
    let was = world.health.get(mark).expect("standing").hp;
    if let Some(at) = world.transform.get_mut(mark) {
        at.pos = bota_proto::Vec2::from_ints(5000 + far, 5000);
    }
    world.step();
    assert_eq!(
        world.attacking.get(attacker).and_then(|a| a.windup),
        None,
        "the swing is given up the moment it gets away"
    );
    assert_eq!(
        world.attacking.get(attacker).map(|a| a.cooldown),
        Some(0),
        "and costs nothing, so the next one may start at once"
    );
    assert_eq!(
        world.health.get(mark).expect("standing").hp,
        was,
        "nothing was struck"
    );
}

#[test]
fn a_swing_is_given_up_when_the_target_falls() {
    let (mut world, attacker, mark) = duel(100);
    world.set_target(attacker, mark);
    world.step();
    assert!(
        world
            .attacking
            .get(attacker)
            .and_then(|a| a.windup)
            .is_some()
    );
    world.health.insert(mark, Health { hp: Fixed::ZERO });
    world.step();
    assert_eq!(
        world.attacking.get(attacker).and_then(|a| a.windup),
        None,
        "there is nothing left to strike"
    );
}

#[test]
fn a_swing_is_given_up_when_the_target_is_lost_from_sight() {
    let (mut world, attacker, mark) = duel(100);
    world.set_target(attacker, mark);
    world.step();
    assert!(
        world
            .attacking
            .get(attacker)
            .and_then(|a| a.windup)
            .is_some()
    );
    // Blinded to it, the way stepping into fog would.
    if let Some(seen) = world.visibility.get_mut(mark) {
        seen.clear();
    }
    swing_once(&mut world);
    assert_eq!(
        world.attacking.get(attacker).and_then(|a| a.windup),
        None,
        "it does not finish a swing at what it can no longer see"
    );
}

/// A hero, an enemy standing in its way, and the order it was given.
fn hero_past_an_enemy(order: crate::engine::UnitOrder) -> (World, Entity, Entity) {
    let mut world = World::new();
    world.seats.push(crate::engine::Seat::new(
        bota_proto::SlotId(0),
        Team::Radiant,
        bota_proto::HeroId(0),
        0,
        rules::STASH_SLOTS,
    ));
    let hero = world.spawn_hero(
        Team::Radiant,
        bota_proto::Vec2::from_ints(5000, 5000),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    let enemy = world.spawn_unit(
        &MELEE_CREEP,
        Team::Dire,
        bota_proto::Vec2::from_ints(5400, 5000),
    );
    world.settle();
    world.seats[0].unit = Some(hero);
    world.orders.insert(
        hero,
        crate::engine::Orders {
            current: order,
            cooldown: 0,
        },
    );
    (world, hero, enemy)
}

#[test]
fn a_hero_told_to_walk_walks_past_what_it_meets() {
    let (mut world, hero, enemy) = hero_past_an_enemy(crate::engine::UnitOrder::Move {
        pos: bota_proto::Vec2::from_ints(7000, 5000),
    });
    let was = world.health.get(enemy).expect("standing").hp;
    for _ in 0..120 {
        world.step();
    }
    assert_eq!(
        world.target_of(hero),
        None,
        "walking somewhere, it takes on nothing"
    );
    assert_eq!(
        world.health.get(enemy).expect("standing").hp,
        was,
        "and strikes nothing"
    );
    let now = world.transform.get(hero).expect("standing").pos;
    assert!(
        now.x.to_int() > 6000,
        "it kept walking where it was sent: {now:?}"
    );
}

#[test]
fn a_hero_told_to_walk_and_attack_stops_for_what_it_meets() {
    let (mut world, hero, enemy) = hero_past_an_enemy(crate::engine::UnitOrder::AttackMove {
        pos: bota_proto::Vec2::from_ints(7000, 5000),
    });
    let was = world.health.get(enemy).expect("standing").hp;
    for _ in 0..120 {
        world.step();
    }
    assert_eq!(world.target_of(hero), Some(enemy), "it took on what it met");
    assert!(
        world.health.get(enemy).expect("standing").hp < was,
        "and struck it"
    );
}

#[test]
fn a_hero_holding_comes_round_but_never_leaves_the_spot() {
    let (mut world, hero, enemy) = hero_past_an_enemy(crate::engine::UnitOrder::Hold);
    // Out of reach, so anything that walked would walk.
    if let Some(at) = world.transform.get_mut(enemy) {
        at.pos = bota_proto::Vec2::from_ints(5500, 5000);
    }
    let stood = world.transform.get(hero).expect("standing").pos;
    for _ in 0..60 {
        world.step();
    }
    assert_eq!(
        world.target_of(hero),
        Some(enemy),
        "holding, it still takes on what comes near"
    );
    assert_eq!(
        world.transform.get(hero).expect("standing").pos,
        stood,
        "but it does not go after it"
    );
}

#[test]
fn a_hero_told_to_stop_stands_and_takes_on_nothing() {
    let (mut world, hero, enemy) = hero_past_an_enemy(crate::engine::UnitOrder::AttackMove {
        pos: bota_proto::Vec2::from_ints(7000, 5000),
    });
    world.step();
    assert_eq!(
        world.target_of(hero),
        Some(enemy),
        "walking to attack, it took the enemy on"
    );
    // The stop key.
    world.advance(&[crate::sim::Command {
        slot: bota_proto::SlotId(0),
        order: bota_proto::Order::Stop,
    }]);
    assert_eq!(
        world.target_of(hero),
        None,
        "stopped, it gives up what it was on"
    );
    let stood = world.transform.get(hero).expect("standing").pos;
    let was = world.health.get(enemy).expect("standing").hp;
    for _ in 0..120 {
        world.step();
    }
    assert_eq!(world.target_of(hero), None, "and takes on nothing more");
    assert_eq!(
        world.health.get(enemy).expect("standing").hp,
        was,
        "so it strikes nothing"
    );
    assert_eq!(
        world.transform.get(hero).expect("standing").pos,
        stood,
        "and keeps the ground it was left on"
    );
}

/// A hero of a side and one of its own creeps beside it.
fn hero_and_own_creep() -> (World, Entity, Entity) {
    let mut world = World::new();
    world.seats.push(crate::engine::Seat::new(
        bota_proto::SlotId(0),
        Team::Radiant,
        bota_proto::HeroId(0),
        0,
        rules::STASH_SLOTS,
    ));
    let hero = world.spawn_hero(
        Team::Radiant,
        bota_proto::Vec2::from_ints(5000, 5000),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    world.seats[0].unit = Some(hero);
    let own = world.spawn_unit(
        &MELEE_CREEP,
        Team::Radiant,
        bota_proto::Vec2::from_ints(5100, 5000),
    );
    world.settle();
    world.fill_pools(hero);
    world.fill_pools(own);
    (world, hero, own)
}

#[test]
fn one_of_your_own_at_full_health_cannot_be_struck() {
    let (mut world, hero, own) = hero_and_own_creep();
    assert!(
        !world.may_attack_on_order(hero, own),
        "a creep at full health is nobody to strike"
    );
    let order = bota_proto::Order::AttackUnit {
        target: crate::engine::wire_id(own),
    };
    assert_eq!(
        world.validate_order(bota_proto::SlotId(0), &order),
        Err(bota_proto::RejectReason::WrongTargetKind),
        "and the order is turned down rather than dropped"
    );
    world.advance(&[crate::sim::Command {
        slot: bota_proto::SlotId(0),
        order,
    }]);
    let was = world.health.get(own).expect("standing").hp;
    for _ in 0..90 {
        world.step();
    }
    assert_eq!(
        world.target_of(hero),
        None,
        "it takes on nothing of its own"
    );
    assert_eq!(
        world.health.get(own).expect("standing").hp,
        was,
        "and strikes nothing"
    );
}

#[test]
fn one_of_your_own_worn_down_far_enough_may_be_put_out() {
    let (mut world, hero, own) = hero_and_own_creep();
    let max = world.stats.get(own).expect("settled").max_hp;
    // A shade under half of what it can hold.
    world.health.insert(
        own,
        Health {
            hp: Fixed {
                raw: max.raw * 49 / 100,
            },
        },
    );
    assert!(
        world.may_attack_on_order(hero, own),
        "worn down, it may be put out"
    );
    let order = bota_proto::Order::AttackUnit {
        target: crate::engine::wire_id(own),
    };
    assert_eq!(world.validate_order(bota_proto::SlotId(0), &order), Ok(()));
    world.advance(&[crate::sim::Command {
        slot: bota_proto::SlotId(0),
        order,
    }]);
    assert_eq!(world.target_of(hero), Some(own), "it takes it on");
    let was = world.health.get(own).expect("standing").hp;
    for _ in 0..90 {
        world.step();
    }
    let now = world
        .health
        .get(own)
        .map_or(Fixed::ZERO, |health| health.hp);
    assert!(now < was, "and strikes it");
}

#[test]
fn a_deny_is_given_up_when_the_creep_is_no_longer_worn_down() {
    let (mut world, hero, own) = hero_and_own_creep();
    let max = world.stats.get(own).expect("settled").max_hp;
    world.health.insert(
        own,
        Health {
            hp: Fixed {
                raw: max.raw * 49 / 100,
            },
        },
    );
    world.advance(&[crate::sim::Command {
        slot: bota_proto::SlotId(0),
        order: bota_proto::Order::AttackUnit {
            target: crate::engine::wire_id(own),
        },
    }]);
    assert_eq!(world.target_of(hero), Some(own));
    // Mended back over the line.
    world.health.insert(own, Health { hp: max });
    world.step();
    assert_eq!(
        world.target_of(hero),
        None,
        "back on its feet, it is nobody to strike again"
    );
}

#[test]
fn your_own_building_goes_only_at_a_tenth() {
    let mut world = World::new();
    world.seats.push(crate::engine::Seat::new(
        bota_proto::SlotId(0),
        Team::Radiant,
        bota_proto::HeroId(0),
        0,
        rules::STASH_SLOTS,
    ));
    let hero = world.spawn_hero(
        Team::Radiant,
        bota_proto::Vec2::from_ints(5000, 5000),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    world.seats[0].unit = Some(hero);
    let tower = world.spawn_unit(
        crate::engine::tower_def(1),
        Team::Radiant,
        bota_proto::Vec2::from_ints(5300, 5000),
    );
    world.settle();
    world.fill_pools(hero);
    world.fill_pools(tower);
    let max = world.stats.get(tower).expect("settled").max_hp;
    assert!(
        !world.may_attack_on_order(hero, tower),
        "standing tall, it is nobody to strike"
    );
    // A tenth is still too much; a shade under is not.
    world.health.insert(
        tower,
        Health {
            hp: Fixed { raw: max.raw / 10 },
        },
    );
    assert!(
        !world.may_attack_on_order(hero, tower),
        "exactly a tenth is not below a tenth"
    );
    world.health.insert(
        tower,
        Health {
            hp: Fixed {
                raw: max.raw * 9 / 100,
            },
        },
    );
    assert!(
        world.may_attack_on_order(hero, tower),
        "worn past it, it may be put out"
    );
    let order = bota_proto::Order::AttackUnit {
        target: crate::engine::wire_id(tower),
    };
    assert_eq!(world.validate_order(bota_proto::SlotId(0), &order), Ok(()));
    world.advance(&[crate::sim::Command {
        slot: bota_proto::SlotId(0),
        order,
    }]);
    assert_eq!(world.target_of(hero), Some(tower));
}

#[test]
fn your_own_hero_is_never_struck_however_worn_down() {
    let mut world = World::new();
    let mine = world.spawn_hero(
        Team::Radiant,
        bota_proto::Vec2::from_ints(5000, 5000),
        bota_proto::SlotId(0),
        bota_proto::HeroId(0),
    );
    let theirs = world.spawn_hero(
        Team::Radiant,
        bota_proto::Vec2::from_ints(5100, 5000),
        bota_proto::SlotId(1),
        bota_proto::HeroId(0),
    );
    world.settle();
    world.health.insert(theirs, Health { hp: Fixed::ONE });
    assert!(
        !world.may_attack_on_order(mine, theirs),
        "one of your own heroes is nobody to strike, worn down or not"
    );
}
