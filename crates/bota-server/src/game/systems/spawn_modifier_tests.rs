//! Trusted setup modifiers: which spawns they take and how long they hold.

use bota_proto::{
    Cheat, DamageKind, EntityId, Fixed, HeroId, MAX_MODIFIER_TICKS, MapId, ModifierSpec, Order,
    Pick, SlotId, Target, Team, TickMode, UnitKind, Vec2,
};

use crate::game::{
    AppliedOrigin, Command, Def, Entity, MAX_SPAWN_MODIFIERS, MatchConfig, MatchConfigError,
    ModifierDuration, SpawnCategory, SpawnModifier, SpawnModifierError, SpawnSelector, SpawnTarget,
    World, rules, wire_id,
};

/// A spec with one change on it, everything else neutral.
fn spec(change: impl FnOnce(&mut ModifierSpec)) -> ModifierSpec {
    let mut spec = ModifierSpec::NOMINAL;
    change(&mut spec);
    spec
}

/// A rule taking everything either side, running until the body falls.
fn a_rule(spec: ModifierSpec) -> SpawnModifier {
    SpawnModifier {
        select: SpawnSelector::default(),
        spec,
        duration: ModifierDuration::MatchLong,
    }
}

/// A one-seat match on the full map with the given rules.
fn config(cheats: bool, rules: Vec<SpawnModifier>) -> MatchConfig {
    MatchConfig {
        match_id: 21,
        master_key: [5; 32],
        picks: vec![Pick {
            slot: SlotId(0),
            team: Team::Radiant,
            hero: HeroId(0),
        }],
        map: MapId(0),
        tick_rate: 30,
        mode: TickMode::Lockstep,
        ack_timeout_ticks: 30,
        cheats,
        spawn_modifiers: rules,
    }
}

/// A world with a hero and a creep, the given rules standing on it.
fn arena(rules: Vec<SpawnModifier>) -> (World, Entity, Entity) {
    let mut world = World::new();
    let hero = world.spawn_hero(
        Team::Radiant,
        Vec2::from_ints(7000, 7000),
        SlotId(0),
        HeroId(0),
    );
    let creep = world.spawn_unit(
        &crate::game::MELEE_CREEP,
        Team::Dire,
        Vec2::from_ints(2000, 2000),
    );
    world.spawn_modifiers = rules;
    world.apply_spawn_modifiers_to_all();
    world.settle();
    (world, hero, creep)
}

/// What a unit's maximum health is raised by, from its definition.
fn raised_hp(world: &World, entity: Entity) -> Fixed {
    let Def(def) = world.def.get(entity).expect("a unit");
    let up = world.upgrades.get(entity).map_or(0, |u| u.0 as i32);
    Fixed::from_int(def.max_hp + up * def.per_upgrade.hp)
}

#[test]
fn a_hero_starts_with_a_match_setup_modifier() {
    let cfg = config(false, vec![a_rule(spec(|s| s.max_hp = 20_000))]);
    let world = World::for_match(&cfg, cfg.rng());
    let hero = world.seats[0].unit.expect("stood up");
    let stats = world.stats.get(hero).expect("settled");
    let expected = rules::HERO_HP * 2 + rules::HP_PER_STRENGTH * 20;
    assert_eq!(stats.max_hp.to_int(), expected);
    assert_eq!(
        world.health.get(hero).map(|health| health.hp),
        Some(stats.max_hp),
        "it stands up full"
    );
    let applied = world.applied.get(hero).expect("seeded");
    assert_eq!(applied.len(), 1);
    assert_eq!(
        applied.first().map(|held| (held.origin, held.ticks_left)),
        Some((AppliedOrigin::Setup, None))
    );
    assert!(cfg.validate().is_ok());
}

#[test]
fn a_hero_on_the_other_side_is_left_alone() {
    let rule = SpawnModifier {
        select: SpawnSelector {
            team: Some(Team::Dire),
            ..SpawnSelector::default()
        },
        spec: spec(|s| s.max_hp = 20_000),
        duration: ModifierDuration::MatchLong,
    };
    let (world, hero, creep) = arena(vec![rule]);
    assert!(!world.applied.contains(hero));
    assert!(world.applied.contains(creep));
    assert_eq!(
        world.stats.get(hero).map(|stats| stats.max_hp),
        Some(Fixed::from_int(
            rules::HERO_HP + rules::HP_PER_STRENGTH * 20
        )),
        "the hero is untouched"
    );
}

#[test]
fn an_exact_unit_selector_takes_only_that_unit() {
    let mut world = World::new();
    let first = world.spawn_hero(
        Team::Radiant,
        Vec2::from_ints(7000, 7000),
        SlotId(0),
        HeroId(0),
    );
    let second = world.spawn_hero(
        Team::Radiant,
        Vec2::from_ints(7600, 7000),
        SlotId(1),
        HeroId(0),
    );
    world.settle();
    world.spawn_modifiers = vec![SpawnModifier {
        select: SpawnSelector {
            unit: Some(wire_id(first)),
            ..SpawnSelector::default()
        },
        spec: spec(|s| s.max_hp = 20_000),
        duration: ModifierDuration::MatchLong,
    }];
    world.apply_spawn_modifiers_to_all();
    world.settle();
    assert!(world.applied.contains(first));
    assert!(!world.applied.contains(second));
}

#[test]
fn an_exact_kind_selector_takes_only_that_kind() {
    let mut world = World::new();
    let melee = world.spawn_unit(
        &crate::game::MELEE_CREEP,
        Team::Dire,
        Vec2::from_ints(2000, 2000),
    );
    let ranged = world.spawn_unit(
        &crate::game::RANGED_CREEP,
        Team::Dire,
        Vec2::from_ints(2200, 2000),
    );
    world.spawn_modifiers = vec![SpawnModifier {
        select: SpawnSelector {
            targets: vec![SpawnTarget::Kind(UnitKind::CreepRanged)],
            ..SpawnSelector::default()
        },
        spec: spec(|s| s.max_hp = 15_000),
        duration: ModifierDuration::MatchLong,
    }];
    world.apply_spawn_modifiers_to_all();
    world.settle();
    assert!(!world.applied.contains(melee));
    assert!(world.applied.contains(ranged));
}

#[test]
fn lane_creeps_are_seeded_on_every_wave() {
    let cfg = config(false, vec![a_rule(spec(|s| s.max_hp = 20_000))]);
    let mut world = World::for_match(&cfg, cfg.rng());
    for wave in 1..=2u32 {
        world.tick = rules::FIRST_WAVE_TICK + (wave - 1) * rules::WAVE_PERIOD_TICKS;
        world.spawn_waves();
        let mut seen = 0;
        for entity in world.entities.iter().collect::<Vec<_>>() {
            if world.kind.get(entity).copied() != Some(UnitKind::CreepMelee) {
                continue;
            }
            let expected = raised_hp(&world, entity);
            assert_eq!(
                world.stats.get(entity).map(|stats| stats.max_hp),
                Some(expected * Fixed::from_int(2)),
                "wave {wave}: a lane creep is raised"
            );
            assert!(
                world.applied.contains(entity),
                "wave {wave}: and carries the rule"
            );
            seen += 1;
        }
        assert!(seen > 0, "wave {wave}: a wave stood up");
    }
}

#[test]
fn neutral_camp_creeps_are_seeded() {
    let cfg = config(false, vec![a_rule(spec(|s| s.max_hp = 20_000))]);
    let mut world = World::for_match(&cfg, cfg.rng());
    world.tick = rules::FIRST_NEUTRAL_TICK;
    world.fill_camps();
    let mut seen = 0;
    for entity in world.entities.iter().collect::<Vec<_>>() {
        if world.kind.get(entity).copied() != Some(UnitKind::CreepNeutral) {
            continue;
        }
        let expected = raised_hp(&world, entity);
        assert_eq!(
            world.stats.get(entity).map(|stats| stats.max_hp),
            Some(expected * Fixed::from_int(2)),
            "a camp creep is raised"
        );
        assert!(world.applied.contains(entity), "and carries the rule");
        seen += 1;
    }
    assert!(seen > 0, "a camp filled");
}

#[test]
fn a_refilled_camp_is_seeded_again() {
    let cfg = config(false, vec![a_rule(spec(|s| s.max_hp = 20_000))]);
    let mut world = World::for_match(&cfg, cfg.rng());
    world.tick = rules::FIRST_NEUTRAL_TICK;
    world.fill_camps();
    let first: Vec<Entity> = world
        .entities
        .iter()
        .filter(|entity| world.kind.get(*entity).copied() == Some(UnitKind::CreepNeutral))
        .collect();
    assert!(!first.is_empty(), "the camp fills");
    for entity in &first {
        world.push_hit(None, *entity, 1_000_000, DamageKind::Pure);
    }
    world.step();
    assert!(
        world
            .entities
            .iter()
            .all(|entity| world.kind.get(entity).copied() != Some(UnitKind::CreepNeutral)),
        "the camp is cleared"
    );
    world.tick = rules::FIRST_NEUTRAL_TICK + rules::NEUTRAL_SPAWN_PERIOD_TICKS;
    world.fill_camps();
    let mut seen = 0;
    for entity in world.entities.iter().collect::<Vec<_>>() {
        if world.kind.get(entity).copied() != Some(UnitKind::CreepNeutral) {
            continue;
        }
        assert!(
            !first.contains(&entity),
            "a refilled camp stands new bodies"
        );
        assert_eq!(
            world.stats.get(entity).map(|stats| stats.max_hp),
            Some(raised_hp(&world, entity) * Fixed::from_int(2)),
            "and each carries the rule"
        );
        assert!(world.applied.contains(entity));
        seen += 1;
    }
    assert!(seen > 0, "the camp fills again");
}

#[test]
fn structures_are_seeded_at_the_start_and_when_stood_up() {
    let rule = SpawnModifier {
        select: SpawnSelector {
            targets: vec![SpawnTarget::Category(SpawnCategory::Structure)],
            ..SpawnSelector::default()
        },
        spec: spec(|s| s.max_hp = 15_000),
        duration: ModifierDuration::MatchLong,
    };
    let cfg = config(false, vec![rule.clone()]);
    let world = World::for_match(&cfg, cfg.rng());
    let tower = world
        .entities
        .iter()
        .find(|entity| world.kind.get(*entity).copied() == Some(UnitKind::Tower))
        .expect("the map stands towers");
    assert!(world.applied.contains(tower), "a standing tower is seeded");
    let expected = world
        .stats
        .get(tower)
        .map(|stats| stats.max_hp)
        .expect("settled");
    assert_eq!(
        expected,
        Fixed {
            raw: (i64::from(raised_hp(&world, tower).raw) * 15_000 / 10_000) as i32
        }
    );

    let mut later = World::new();
    later.spawn_modifiers = vec![rule];
    let standing = later.spawn_building(
        crate::game::tower_def(1),
        Team::Dire,
        Vec2::from_ints(5000, 5000),
        crate::game::Place::Tower { lane: 0, tier: 1 },
    );
    later.settle();
    assert!(later.applied.contains(standing), "a later tower is seeded");
    assert_eq!(
        later.stats.get(standing).map(|stats| stats.max_hp.to_int()),
        Some(crate::game::tower_def(1).max_hp * 15_000 / 10_000)
    );
}

#[test]
fn several_rules_add_their_scales_once() {
    let cfg = config(
        false,
        vec![
            a_rule(spec(|s| s.move_speed = 11_000)),
            a_rule(spec(|s| s.move_speed = 12_000)),
            a_rule(spec(|s| s.max_hp = 12_500)),
            a_rule(spec(|s| s.max_hp = 12_500)),
        ],
    );
    let world = World::for_match(&cfg, cfg.rng());
    let hero = world.seats[0].unit.expect("stood up");
    let stats = world.stats.get(hero).expect("settled");
    assert_eq!(
        stats.move_speed.to_int(),
        rules::HERO_MOVE_SPEED * 13 / 10,
        "deltas add: 10% and 20% make 30%, not 32%"
    );
    assert_eq!(
        stats.max_hp.to_int(),
        rules::HERO_HP * 15 / 10 + rules::HP_PER_STRENGTH * 20,
        "the two quarter shares make one half"
    );
}

#[test]
fn a_ticks_rule_lifts_after_its_ticks() {
    let cfg = config(
        false,
        vec![SpawnModifier {
            select: SpawnSelector::default(),
            spec: spec(|s| s.max_hp = 20_000),
            duration: ModifierDuration::Ticks(2),
        }],
    );
    let mut world = World::for_match(&cfg, cfg.rng());
    let hero = world.seats[0].unit.expect("stood up");
    assert_eq!(
        world
            .applied
            .get(hero)
            .and_then(|applied| applied.first())
            .and_then(|held| held.ticks_left),
        Some(2)
    );
    world.step();
    assert!(world.applied.contains(hero), "one tick left");
    world.step();
    assert!(!world.applied.contains(hero), "it has run out");
    world.step();
    assert_eq!(
        world.stats.get(hero).map(|stats| stats.max_hp.to_int()),
        Some(rules::HERO_HP + rules::HP_PER_STRENGTH * 20),
        "and the maximum is back"
    );
}

#[test]
fn a_respawned_hero_is_seeded_afresh_and_stands_full() {
    let cfg = config(false, vec![a_rule(spec(|s| s.max_hp = 20_000))]);
    let mut world = World::for_match(&cfg, cfg.rng());
    let hero = world.seats[0].unit.expect("stood up");
    let full = world.stats.get(hero).expect("settled").max_hp;
    assert_eq!(
        full.to_int(),
        rules::HERO_HP * 2 + rules::HP_PER_STRENGTH * 20
    );
    world.push_hit(None, hero, 1_000_000, DamageKind::Pure);
    world.step();
    assert!(!world.alive(hero), "the blow was fatal");
    let wait = world.seats[0].respawn_left;
    assert!(wait > 0);
    for _ in 0..=wait {
        world.step();
    }
    let reborn = world.seats[0].unit.expect("the hero comes back");
    let stats = *world.stats.get(reborn).expect("settled");
    assert_eq!(stats.max_hp, full, "the rule is on the new body");
    assert_eq!(
        world.health.get(reborn).map(|health| health.hp),
        Some(stats.max_hp),
        "and it stands at that maximum, not below it"
    );
    assert_eq!(
        world
            .applied
            .get(reborn)
            .and_then(|applied| applied.first()),
        Some(&crate::game::AppliedModifier {
            spec: spec(|s| s.max_hp = 20_000),
            ticks_left: None,
            origin: AppliedOrigin::Setup,
        })
    );
}

#[test]
fn a_respawned_hero_never_stands_above_a_shrunk_maximum() {
    let cfg = config(false, vec![a_rule(spec(|s| s.max_hp = 5_000))]);
    let mut world = World::for_match(&cfg, cfg.rng());
    let hero = world.seats[0].unit.expect("stood up");
    let shrunken = rules::HERO_HP / 2 + rules::HP_PER_STRENGTH * 20;
    assert_eq!(
        world.stats.get(hero).map(|stats| stats.max_hp.to_int()),
        Some(shrunken)
    );
    world.push_hit(None, hero, 1_000_000, DamageKind::Pure);
    world.step();
    let wait = world.seats[0].respawn_left;
    for _ in 0..=wait {
        world.step();
    }
    let reborn = world.seats[0].unit.expect("the hero comes back");
    let stats = *world.stats.get(reborn).expect("settled");
    assert_eq!(
        stats.max_hp.to_int(),
        shrunken,
        "the rule is on the new body"
    );
    assert_eq!(
        world.health.get(reborn).map(|health| health.hp),
        Some(stats.max_hp),
        "full to the shrunk maximum and no further"
    );
}

#[test]
fn an_empty_rule_list_leaves_the_world_alone() {
    let cfg = config(false, Vec::new());
    let world = World::for_match(&cfg, cfg.rng());
    assert!(world.spawn_modifiers.is_empty());
    assert!(world.applied.is_empty(), "nothing was seeded");
    let run = || {
        let cfg = config(false, Vec::new());
        let mut world = World::for_match(&cfg, cfg.rng());
        for _ in 0..200 {
            world.step();
        }
        world.hash()
    };
    assert_eq!(run(), run(), "two empty setups go the same way");
}

#[test]
fn two_runs_with_the_same_rules_hash_the_same() {
    let run = || {
        let cfg = config(
            false,
            vec![
                a_rule(spec(|s| s.max_hp = 12_500)),
                a_rule(spec(|s| s.move_speed = 9_000)),
            ],
        );
        let mut world = World::for_match(&cfg, cfg.rng());
        for _ in 0..300 {
            world.step();
        }
        world.hash()
    };
    assert_eq!(run(), run(), "the same rules, the same world");
}

#[test]
fn invalid_rules_are_refused_at_setup() {
    let mut cfg = config(false, vec![a_rule(ModifierSpec::NOMINAL)]);
    cfg.spawn_modifiers[0].spec.status_resist = ModifierSpec::MAX_STATUS_RESIST + 1;
    assert_eq!(
        cfg.validate(),
        Err(MatchConfigError::SpawnModifier {
            at: 0,
            error: SpawnModifierError::UnboundedSpec,
        })
    );
    cfg.spawn_modifiers[0].spec = ModifierSpec::NOMINAL;
    cfg.spawn_modifiers[0].duration = ModifierDuration::Ticks(0);
    assert_eq!(
        cfg.validate(),
        Err(MatchConfigError::SpawnModifier {
            at: 0,
            error: SpawnModifierError::BadDuration,
        })
    );
    cfg.spawn_modifiers[0].duration = ModifierDuration::Ticks(MAX_MODIFIER_TICKS + 1);
    assert_eq!(
        cfg.validate(),
        Err(MatchConfigError::SpawnModifier {
            at: 0,
            error: SpawnModifierError::BadDuration,
        })
    );
    cfg.spawn_modifiers = vec![a_rule(ModifierSpec::NOMINAL); MAX_SPAWN_MODIFIERS + 1];
    assert_eq!(
        cfg.validate(),
        Err(MatchConfigError::TooManySpawnModifiers {
            count: MAX_SPAWN_MODIFIERS + 1,
        })
    );
    cfg.spawn_modifiers = vec![a_rule(ModifierSpec::NOMINAL); MAX_SPAWN_MODIFIERS];
    assert_eq!(cfg.validate(), Ok(()), "the bound itself is allowed");
}

#[test]
fn a_cheat_does_not_take_trusted_setup_off_a_unit() {
    let cfg = config(true, vec![a_rule(spec(|s| s.max_hp = 20_000))]);
    let mut world = World::for_match(&cfg, cfg.rng());
    let hero = world.seats[0].unit.expect("stood up");
    let cheat = |cheat: Cheat| Command {
        slot: SlotId(0),
        unit: None,
        order: Order::Cheat { cheat },
    };
    world.advance(&[cheat(Cheat::ApplyModifier {
        target: Target::None,
        spec: spec(|s| s.status_resist = 2_000),
        ticks: 50,
    })]);
    let applied = world.applied.get(hero).expect("both are on it");
    assert_eq!(applied.len(), 2);
    assert!(
        applied
            .iter()
            .any(|held| held.origin == AppliedOrigin::Setup)
    );
    assert!(
        applied
            .iter()
            .any(|held| held.origin == AppliedOrigin::Cheat)
    );
    world.advance(&[cheat(Cheat::ClearModifiers {
        target: Target::None,
    })]);
    let applied = world.applied.get(hero).expect("setup stays");
    assert_eq!(applied.len(), 1);
    assert_eq!(
        applied.first().map(|held| held.origin),
        Some(AppliedOrigin::Setup)
    );
}

#[test]
fn an_unknown_explicit_unit_selector_takes_nothing() {
    let cfg = config(
        false,
        vec![SpawnModifier {
            select: SpawnSelector {
                unit: Some(EntityId {
                    idx: 999,
                    generation: 7,
                }),
                ..SpawnSelector::default()
            },
            spec: spec(|s| s.max_hp = 20_000),
            duration: ModifierDuration::MatchLong,
        }],
    );
    let world = World::for_match(&cfg, cfg.rng());
    let hero = world.seats[0].unit.expect("stood up");
    assert!(!world.applied.contains(hero));
}
