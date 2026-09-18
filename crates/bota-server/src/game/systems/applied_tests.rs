//! Cheat-granted stat changes: what each is worth and what it ignores.

use bota_proto::{
    AbilitySlot, DamageKind, Fixed, HeroId, ItemId, ModifierSpec, SlotId, Target, Team, UnitKind,
    Vec2,
};

use crate::game::{
    AppliedModifier, Entity, ITEMS, ItemStack, MELEE_CREEP, Modifier, ModifierKind, World, ability,
    ability_cooldown, ability_mana_cost, rules, wire_id,
};

/// A world with a hero and a creep standing well apart.
fn arena() -> (World, Entity, Entity) {
    let mut world = World::new();
    let hero = world.spawn_hero(
        Team::Radiant,
        Vec2::from_ints(7000, 7000),
        SlotId(0),
        HeroId(0),
    );
    let creep = world.spawn_unit(&MELEE_CREEP, Team::Dire, Vec2::from_ints(1000, 1000));
    world.settle();
    (world, hero, creep)
}

/// One nominal spec with `change` applied to it.
fn spec(change: impl FnOnce(&mut ModifierSpec)) -> ModifierSpec {
    let mut spec = ModifierSpec::NOMINAL;
    change(&mut spec);
    spec
}

/// Puts a spec on a unit and works its stats out again.
fn apply(world: &mut World, on: Entity, spec: ModifierSpec, ticks: u32) {
    world.applied.insert(
        on,
        AppliedModifier {
            spec,
            ticks_left: ticks,
        },
    );
    world.settle();
}

/// Damage one blow of a kind takes off a target, through a whole tick.
fn dealt(world: &mut World, from: Entity, to: Entity, amount: i32, kind: DamageKind) -> i32 {
    let before = world.health.get(to).expect("standing").hp;
    world.push_hit(Some(from), to, amount, kind);
    world.step();
    (before - world.health.get(to).expect("standing").hp).to_int()
}

/// A stun of `ticks` ticks.
fn stun(ticks: u32) -> Modifier {
    Modifier {
        kind: ModifierKind::Stunned,
        source: None,
        ticks_left: Some(ticks),
    }
}

/// What a modifier of one kind has left on a unit.
fn left_on(world: &World, on: Entity, kind: ModifierKind) -> Option<u32> {
    world
        .modifiers
        .get(on)?
        .active()
        .find(|held| held.kind == kind)?
        .ticks_left
}

/// Puts an item in the first bag slot of a unit.
fn give(world: &mut World, on: Entity, item: u16) {
    let tick = world.tick;
    let Some(stack) = ItemStack::bought(ItemId(item), SlotId(0), tick) else {
        return;
    };
    if let Some(bag) = world.inventory.get_mut(on) {
        bag.slots[0] = Some(stack);
    }
}

#[test]
fn neutral_stats_leave_damage_alone() {
    let (mut world, hero, creep) = arena();
    apply(&mut world, hero, ModifierSpec::NOMINAL, 10);
    assert_eq!(dealt(&mut world, hero, creep, 100, DamageKind::Pure), 100);
    assert_eq!(dealt(&mut world, hero, creep, 64, DamageKind::Magical), 64);
}

#[test]
fn magic_resistance_delta_reduces_magical_damage_only() {
    let (mut world, hero, creep) = arena();
    apply(&mut world, creep, spec(|s| s.magic_resist = 5_000), 10);
    assert_eq!(dealt(&mut world, hero, creep, 100, DamageKind::Magical), 50);
    assert_eq!(dealt(&mut world, hero, creep, 100, DamageKind::Pure), 100);
}

#[test]
fn magic_resistance_delta_is_clamped_at_total_immunity() {
    let (mut world, hero, creep) = arena();
    apply(&mut world, creep, spec(|s| s.magic_resist = 10_000), 10);
    assert_eq!(
        world.stats.get(creep).map(|stats| stats.magic_resist_pct),
        Some(100),
        "the delta may not take it past immunity"
    );
    assert_eq!(dealt(&mut world, hero, creep, 100, DamageKind::Magical), 0);
}

#[test]
fn magical_amplification_scales_magical_damage() {
    let (mut world, hero, creep) = arena();
    apply(&mut world, hero, spec(|s| s.magic_damage = 15_000), 10);
    assert_eq!(
        dealt(&mut world, hero, creep, 100, DamageKind::Magical),
        150
    );
}

#[test]
fn magical_amplification_applies_before_magic_resistance() {
    let (mut world, hero, creep) = arena();
    apply(&mut world, hero, spec(|s| s.magic_damage = 20_000), 10);
    apply(&mut world, creep, spec(|s| s.magic_resist = 5_000), 10);
    assert_eq!(
        dealt(&mut world, hero, creep, 100, DamageKind::Magical),
        100,
        "double the blow, then half of it kept"
    );
}

#[test]
fn physical_amplification_scales_physical_damage() {
    let (mut world, hero, creep) = arena();
    let neutral = dealt(&mut world, hero, creep, 100, DamageKind::Physical);
    assert!(neutral > 0);
    let (mut world, hero, creep) = arena();
    apply(&mut world, hero, spec(|s| s.physical_damage = 20_000), 10);
    assert_eq!(
        dealt(&mut world, hero, creep, 100, DamageKind::Physical),
        2 * neutral
    );
}

#[test]
fn pure_amplification_scales_pure_damage() {
    let (mut world, hero, creep) = arena();
    let neutral = dealt(&mut world, hero, creep, 100, DamageKind::Pure);
    let (mut world, hero, creep) = arena();
    apply(&mut world, hero, spec(|s| s.pure_damage = 15_000), 10);
    assert_eq!(dealt(&mut world, hero, creep, 100, DamageKind::Pure), 150);
    assert_eq!(neutral, 100);
}

#[test]
fn amplification_of_one_kind_leaves_the_other_kinds_alone() {
    let (mut world, hero, creep) = arena();
    apply(&mut world, hero, spec(|s| s.physical_damage = 20_000), 10);
    assert_eq!(
        dealt(&mut world, hero, creep, 100, DamageKind::Magical),
        100
    );
    assert_eq!(dealt(&mut world, hero, creep, 100, DamageKind::Pure), 100);
}

#[test]
fn neutral_status_resistance_leaves_durations_alone() {
    let (mut world, hero, _creep) = arena();
    apply(&mut world, hero, ModifierSpec::NOMINAL, 10);
    world.put_modifier(hero, stun(10));
    assert_eq!(left_on(&world, hero, ModifierKind::Stunned), Some(10));
}

#[test]
fn status_resistance_shortens_stuns_fears_and_slows() {
    let (mut world, hero, _creep) = arena();
    apply(&mut world, hero, spec(|s| s.status_resist = 5_000), 10);
    for kind in [
        ModifierKind::Stunned,
        ModifierKind::Feared,
        ModifierKind::Slowed { pct: 30 },
    ] {
        world.put_modifier(
            hero,
            Modifier {
                kind,
                source: None,
                ticks_left: Some(10),
            },
        );
        assert_eq!(left_on(&world, hero, kind), Some(5), "{kind:?}");
        world.take_modifier(hero, kind, None);
    }
    world.put_modifier(
        hero,
        Modifier {
            kind: ModifierKind::Haste { speed: 10 },
            source: None,
            ticks_left: Some(10),
        },
    );
    assert_eq!(
        left_on(&world, hero, ModifierKind::Haste { speed: 10 }),
        Some(10),
        "a buff is not a disable"
    );
}

#[test]
fn status_resistance_never_shortens_a_disable_below_one_tick() {
    let (mut world, hero, _creep) = arena();
    apply(&mut world, hero, spec(|s| s.status_resist = 9_000), 10);
    world.put_modifier(hero, stun(3));
    assert_eq!(left_on(&world, hero, ModifierKind::Stunned), Some(1));
}

#[test]
fn status_resistance_shortens_the_stretch_extended_by_once() {
    let (mut world, hero, _creep) = arena();
    apply(&mut world, hero, spec(|s| s.status_resist = 5_000), 10);
    world.put_modifier(hero, stun(10));
    assert_eq!(left_on(&world, hero, ModifierKind::Stunned), Some(5));
    world.extend_modifier(hero, stun(10), 100);
    assert_eq!(
        left_on(&world, hero, ModifierKind::Stunned),
        Some(10),
        "what is already held is not shortened a second time"
    );
}

#[test]
fn neutral_rates_leave_costs_and_cooldowns_alone() {
    let (mut world, hero, _creep) = arena();
    apply(&mut world, hero, ModifierSpec::NOMINAL, 10);
    assert_eq!(
        world.ability_mana_cost(hero, ability::FRENZY, 1),
        ability_mana_cost(ability::FRENZY, 1)
    );
    assert_eq!(
        world.ability_cooldown(hero, ability::FRENZY, 1),
        ability_cooldown(ability::FRENZY, 1)
    );
}

#[test]
fn mana_cost_rate_is_charged_and_shown() {
    let (mut world, hero, _creep) = arena();
    if let Some(book) = world.abilities.get_mut(hero) {
        book.slots[1].level = 1;
    }
    apply(&mut world, hero, spec(|s| s.mana_cost_rate = 5_000), 10);
    let base = ability_mana_cost(ability::FRENZY, 1);
    assert_eq!(base, rules::SYLLA_FRENZY_MANA[0]);
    assert_eq!(world.ability_mana_cost(hero, ability::FRENZY, 1), base / 2);
    let view = world.view_full();
    let unit = view
        .units
        .iter()
        .find(|unit| unit.id == wire_id(hero))
        .expect("the hero is in the view");
    assert_eq!(
        unit.abilities[1].mana_cost,
        base / 2,
        "the projection shows what a cast pays"
    );
    let before = world.mana.get(hero).expect("has mana").mana;
    assert!(world.begin_ability(hero, AbilitySlot(1), Target::None));
    assert_eq!(
        before - world.mana.get(hero).expect("has mana").mana,
        Fixed::from_int(base / 2),
        "the cast charged the scaled cost"
    );
}

#[test]
fn cooldown_rate_shortens_what_a_cast_sets() {
    let (mut world, hero, _creep) = arena();
    if let Some(book) = world.abilities.get_mut(hero) {
        book.slots[1].level = 1;
    }
    apply(&mut world, hero, spec(|s| s.cooldown_rate = 5_000), 10);
    assert!(world.begin_ability(hero, AbilitySlot(1), Target::None));
    assert_eq!(
        world
            .abilities
            .get(hero)
            .and_then(|book| book.slots.get(1))
            .map(|slot| slot.cooldown),
        Some(ability_cooldown(ability::FRENZY, 1) / 2)
    );
}

#[test]
fn cooldown_rate_shortens_what_an_item_use_sets() {
    let (mut world, hero, _creep) = arena();
    give(&mut world, hero, crate::game::ITEM_MAGIC_STICK);
    if let Some(stack) = world
        .inventory
        .get_mut(hero)
        .and_then(|bag| bag.slots[0].as_mut())
    {
        stack.charges = 1;
    }
    apply(&mut world, hero, spec(|s| s.cooldown_rate = 5_000), 10);
    assert!(world.begin_item(hero, 0, Target::None));
    let full = ITEMS[usize::from(crate::game::ITEM_MAGIC_STICK)].cooldown;
    assert_eq!(
        world
            .inventory
            .get(hero)
            .and_then(|bag| bag.slots[0])
            .map(|stack| stack.cooldown),
        Some(full / 2)
    );
}

#[test]
fn a_blow_sets_a_muted_item_back_by_the_cooldown_rate() {
    let (mut world, hero, _creep) = arena();
    give(&mut world, hero, crate::game::ITEM_BLINK_DAGGER);
    apply(&mut world, hero, spec(|s| s.cooldown_rate = 5_000), 10);
    let hitter = world.spawn();
    world.kind.insert(hitter, UnitKind::Hero);
    world.push_hit(Some(hitter), hero, 10, DamageKind::Physical);
    world.step();
    let full = ITEMS[usize::from(crate::game::ITEM_BLINK_DAGGER)].breaks_on_damage;
    assert_eq!(
        world
            .inventory
            .get(hero)
            .and_then(|bag| bag.slots[0])
            .map(|stack| stack.cooldown),
        Some(full / 2)
    );
}

#[test]
fn a_cheat_modifier_is_not_an_effect_on_the_wire() {
    let (mut world, hero, _creep) = arena();
    let effects = |world: &World| {
        world
            .view_full()
            .units
            .into_iter()
            .find(|unit| unit.id == wire_id(hero))
            .expect("the hero is in the view")
            .effects
    };
    let before = effects(&world);
    apply(&mut world, hero, spec(|s| s.status_resist = 1_000), 10);
    assert_eq!(effects(&world), before, "nothing new shows on the unit");
}

#[test]
fn nothing_a_normal_modifier_does_reaches_a_cheat_modifier() {
    let (mut world, hero, _creep) = arena();
    apply(&mut world, hero, spec(|s| s.magic_resist = 1_000), 100);
    world.put_modifier(hero, stun(10));
    world.take_modifier(hero, ModifierKind::Stunned, None);
    if let Some(on_it) = world.modifiers.get_mut(hero) {
        // A purge of everything the modifier system holds.
        on_it.0.clear();
    }
    for _ in 0..10 {
        world.tick_gear();
    }
    assert!(
        world.applied.contains(hero),
        "only the countdown may take it away"
    );
    assert_eq!(
        world.stats.get(hero).map(|stats| stats.magic_resist_pct),
        Some(35),
        "and its stat still applies"
    );
}
