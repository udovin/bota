//! Cheats: what each one does for a seat, standing or fallen.

use bota_proto::{EventKind, Fixed, HeroId, ItemId, MapId, Pick, Rules, SlotId, Team, TickMode};

use crate::game::{
    Cheat, Entity, Event, ITEM_BUTTERFLY, ItemStack, MatchConfig, STANDARD_RULES, World, rules,
};

/// A one-seat match on the demo map with cheats on.
fn config() -> MatchConfig {
    MatchConfig {
        match_id: 5,
        master_key: [9; 32],
        picks: vec![Pick {
            slot: SlotId(0),
            team: Team::Radiant,
            hero: HeroId(0),
        }],
        map: MapId(1),
        tick_rate: 30,
        mode: TickMode::Lockstep,
        ack_timeout_ticks: 30,
        rules: Rules {
            cheats: true,
            ..STANDARD_RULES
        },
    }
}

/// A world with cheats on and its one hero.
fn cheating_world() -> (World, Entity) {
    let cfg = config();
    let world = World::for_match(&cfg, cfg.rng());
    let hero = world.seats[0].unit.expect("stood up");
    (world, hero)
}

/// Takes one cheat for the seat.
fn cheat(world: &mut World, cheat: Cheat) -> Vec<Event> {
    let mut events = Vec::new();
    world.cheat(SlotId(0), cheat, &mut events);
    events
}

#[test]
fn gold_comes_out_of_nowhere_and_never_goes_below_nothing() {
    let (mut world, _hero) = cheating_world();
    let before = world.seats[0].gold;
    cheat(&mut world, Cheat::Gold { amount: 500 });
    assert_eq!(world.seats[0].gold, before + 500);
    cheat(&mut world, Cheat::Gold { amount: -10_000 });
    assert_eq!(world.seats[0].gold, 0, "taking away stops at nothing");
}

#[test]
fn levels_go_up_at_once_and_no_further_than_the_cap() {
    let (mut world, hero) = cheating_world();
    let events = cheat(&mut world, Cheat::Levels { count: 3 });
    assert_eq!(world.seats[0].level, 4);
    assert_eq!(
        world.level.get(hero).map(|level| level.0),
        Some(4),
        "the body is raised with the seat"
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.kind, EventKind::LevelUp { .. }))
            .count(),
        3,
        "every level passed is told of"
    );
    cheat(&mut world, Cheat::Levels { count: 100 });
    assert_eq!(world.seats[0].level, rules::HERO_MAX_LEVEL);
}

#[test]
fn a_refresh_fills_the_pools_and_clears_every_wait() {
    let (mut world, hero) = cheating_world();
    world.health.get_mut(hero).expect("standing").hp = Fixed::from_int(1);
    world.mana.get_mut(hero).expect("has mana").mana = Fixed::ZERO;
    world.abilities.get_mut(hero).expect("has a book").slots[1].cooldown = 100;
    world.inventory.get_mut(hero).expect("has a bag").slots[0] = Some(ItemStack {
        id: ItemId(crate::game::ITEM_TOWN_PORTAL_SCROLL),
        charges: 1,
        cooldown: 50,
        mute: 20,
        mode: None,
        bought_tick: 0,
        touched: false,
        owner: SlotId(0),
        for_sale: false,
    });
    world.seats[0]
        .item_clocks
        .push((ItemId(crate::game::ITEM_TOWN_PORTAL_SCROLL), 900));
    cheat(&mut world, Cheat::Refresh);
    let stats = *world.stats.get(hero).expect("settled");
    assert_eq!(world.health.get(hero).map(|h| h.hp), Some(stats.max_hp));
    assert_eq!(world.mana.get(hero).map(|m| m.mana), Some(stats.max_mana));
    assert_eq!(
        world.abilities.get(hero).map(|book| book.slots[1].cooldown),
        Some(0)
    );
    let scroll = world.inventory.get(hero).expect("has a bag").slots[0].expect("still held");
    assert_eq!((scroll.cooldown, scroll.mute), (0, 0));
    assert!(world.seats[0].item_clocks.is_empty());
}

#[test]
fn an_item_is_handed_out_for_nothing_and_told_of_as_bought() {
    let (mut world, hero) = cheating_world();
    let gold = world.seats[0].gold;
    let events = cheat(
        &mut world,
        Cheat::Item {
            item: ItemId(ITEM_BUTTERFLY),
        },
    );
    assert!(
        world
            .inventory
            .get(hero)
            .expect("has a bag")
            .held()
            .any(|stack| stack.id == ItemId(ITEM_BUTTERFLY)),
        "the Butterfly is in the bag"
    );
    assert_eq!(world.seats[0].gold, gold, "and nothing was paid");
    assert!(events.iter().any(|event| matches!(
        event.kind,
        EventKind::ItemBought {
            item: ItemId(ITEM_BUTTERFLY),
            ..
        }
    )));
}

#[test]
fn a_fallen_hero_levels_and_gets_items_but_is_not_refreshed() {
    let (mut world, hero) = cheating_world();
    world.bury(vec![(hero, None)], &mut Vec::new());
    assert_eq!(world.seats[0].fallen, Some(hero));

    cheat(&mut world, Cheat::Refresh);
    assert_eq!(
        world.health.get(hero).map(|h| h.hp),
        Some(Fixed::ZERO),
        "a body lying fallen is left as it lies"
    );

    cheat(&mut world, Cheat::Levels { count: 2 });
    assert_eq!(world.seats[0].level, 3);
    assert_eq!(
        world.level.get(hero).map(|level| level.0),
        Some(3),
        "the fallen body has the points to spend at once"
    );

    cheat(
        &mut world,
        Cheat::Item {
            item: ItemId(ITEM_BUTTERFLY),
        },
    );
    assert!(
        world.seats[0]
            .stash
            .held()
            .any(|stack| stack.id == ItemId(ITEM_BUTTERFLY)),
        "what a fallen hero is given waits in the stash"
    );
}
