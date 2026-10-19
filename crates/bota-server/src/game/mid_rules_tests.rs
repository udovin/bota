//! The mid rules on the Dota map: waves in mid alone, the side towers out of
//! reach, and the end of a match judged once a tick.

use bota_proto::{
    DamageKind, EventKind, FromGame, HeroId, ItemId, MapId, Order, Pick, ReplayRecord, Rules,
    SlotId, Team, TickMode, UnitKind, Vec2, decode_payload, encode_frame_to_vec,
};

use crate::game::{
    Command, Entity, Event, EventVisibility, MID_RULES, MatchConfig, STANDARD_RULES, Seat, World,
    map_of, rules, wave_plan, wire_id,
};

const TICK_CAP: u32 = 27_900;
const FINAL_WAVE: u32 = (TICK_CAP - rules::FIRST_WAVE_TICK) / rules::WAVE_PERIOD_TICKS + 1;

fn config(map: MapId, ruled: Rules) -> MatchConfig {
    MatchConfig {
        match_id: 23,
        master_key: [17; 32],
        picks: vec![
            Pick {
                slot: SlotId(0),
                team: Team::Radiant,
                hero: HeroId(0),
            },
            Pick {
                slot: SlotId(1),
                team: Team::Dire,
                hero: HeroId(0),
            },
        ],
        map,
        tick_rate: 30,
        mode: TickMode::Lockstep,
        ack_timeout_ticks: 150,
        rules: ruled,
    }
}

fn match_world(map: MapId, ruled: Rules) -> World {
    let config = config(map, ruled);
    let world = World::for_match(&config, config.rng());
    assert_eq!(world.seats.len(), 2);
    assert_eq!(world.victor(), None);
    world
}

/// A one-on-one on the Dota map under the mid rules.
fn mid_match() -> World {
    match_world(MapId(0), MID_RULES)
}

/// A world with nothing on it yet but the Dota map and the mid rules.
fn mid_ground() -> World {
    let mut world = World::on_map(map_of(MapId(0)));
    world.rules = MID_RULES;
    world
}

fn tower(world: &World, team: Team, lane: u8, tier: u8) -> Entity {
    world
        .entities
        .iter()
        .find(|&entity| {
            world.kind.get(entity) == Some(&UnitKind::Tower)
                && world.team.get(entity) == Some(&team)
                && world.lane.get(entity).is_some_and(|value| value.0 == lane)
                && world.tier.get(entity).is_some_and(|value| value.0 == tier)
        })
        .expect("a tower on the requested lane and tier")
}

fn lethal_hit(world: &mut World, victim: Entity) {
    assert!(world.alive(victim));
    assert!(
        !world
            .stats
            .get(victim)
            .expect("settled victim")
            .invulnerable
    );
    world.push_hit(None, victim, 30_000, DamageKind::Pure);
}

fn assert_destroyed(events: &[Event], victim: Entity, team: Team) {
    let destroyed = events
        .iter()
        .filter(|event| {
            event.kind
                == EventKind::StructureDestroyed {
                    unit: wire_id(victim),
                    team,
                }
        })
        .collect::<Vec<_>>();
    assert_eq!(destroyed.len(), 1);
    assert_eq!(destroyed[0].visible_to, EventVisibility::Everyone);
}

fn lane_creeps(world: &World) -> usize {
    world
        .entities
        .iter()
        .filter(|&entity| world.march.get(entity).is_some())
        .count()
}

#[test]
fn waves_walk_mid_alone_for_both_sides_through_the_time_limit() {
    for wave in 1..=FINAL_WAVE {
        let mut world = mid_ground();
        world.tick = rules::FIRST_WAVE_TICK + (wave - 1) * rules::WAVE_PERIOD_TICKS;
        let plan = wave_plan(wave);
        let per_side = (plan.melee + plan.ranged + plan.siege) as usize;

        world.spawn_waves();

        for team in [Team::Radiant, Team::Dire] {
            let creeps = world
                .entities
                .iter()
                .filter(|&entity| {
                    world.march.get(entity).is_some() && world.team.get(entity) == Some(&team)
                })
                .collect::<Vec<_>>();
            assert_eq!(creeps.len(), per_side, "wave {wave}, {team:?}");
            for creep in creeps {
                assert_eq!(
                    world.lane.get(creep).map(|lane| lane.0),
                    Some(rules::LANE_MID)
                );
            }
        }
    }
}

#[test]
fn no_lane_creeps_come_before_the_first_wave_or_between_waves() {
    for tick in [0, rules::FIRST_WAVE_TICK - 1, rules::FIRST_WAVE_TICK + 1] {
        let mut world = mid_ground();
        world.tick = tick;

        world.spawn_waves();

        assert_eq!(lane_creeps(&world), 0);
    }
}

#[test]
fn the_side_towers_take_no_harm_and_mid_stands_open() {
    let mut world = mid_match();
    for team in [Team::Radiant, Team::Dire] {
        for lane in [rules::LANE_TOP, rules::LANE_BOT] {
            for tier in 1..=3 {
                let kept = tower(&world, team, lane, tier);
                assert!(
                    world.stats.get(kept).expect("settled").invulnerable,
                    "{team:?} lane {lane} tier {tier}"
                );
            }
        }
        let open = tower(&world, team, rules::LANE_MID, 1);
        assert!(!world.stats.get(open).expect("settled").invulnerable);
    }
    let kept = tower(&world, Team::Dire, rules::LANE_TOP, 1);
    world.push_hit(None, kept, 30_000, DamageKind::Pure);
    world.advance(&[]);
    assert!(world.alive(kept), "a blow at it lands on nothing");
    assert_eq!(world.victor(), None);
}

#[test]
fn a_first_hero_death_allows_a_respawn_and_a_second_loses() {
    for (seat, winner) in [(0, Team::Dire), (1, Team::Radiant)] {
        let mut world = mid_match();
        let first = world.seats[seat].unit.expect("first life");
        lethal_hit(&mut world, first);

        world.advance(&[]);

        assert_eq!(world.seats[seat].deaths, 1);
        assert_eq!(world.victor(), None);
        assert!(world.seats[seat].unit.is_none());
        world.seats[seat].respawn_left = 1;
        world.advance(&[]);
        let second = world.seats[seat].unit.expect("second life respawned");
        assert_eq!(second, first, "the same body stands up for its second life");
        lethal_hit(&mut world, second);
        world.advance(&[]);
        assert_eq!(world.seats[seat].deaths, 2);
        assert_eq!(world.victor(), Some(winner));
    }
}

#[test]
fn hero_lives_are_counted_per_side_across_seats() {
    let mut world = mid_match();
    let slot = SlotId(2);
    let mut teammate = Seat::new(slot, Team::Radiant, HeroId(0), 0, rules::STASH_SLOTS);
    teammate.unit =
        Some(world.spawn_hero(Team::Radiant, Vec2::from_ints(5000, 5000), slot, HeroId(0)));
    teammate.deaths = 1;
    world.seats.push(teammate);
    let victim = world.seats[0].unit.expect("Radiant hero");
    lethal_hit(&mut world, victim);

    world.advance(&[]);

    assert_eq!(world.seats[0].deaths, 1);
    assert_eq!(world.seats[2].deaths, 1);
    assert_eq!(world.victor(), Some(Team::Dire));
}

#[test]
fn the_first_mid_tower_lost_ends_it_with_a_public_structure_reason() {
    for (loser, winner) in [(Team::Radiant, Team::Dire), (Team::Dire, Team::Radiant)] {
        let mut world = mid_match();
        let victim = tower(&world, loser, rules::LANE_MID, 1);
        lethal_hit(&mut world, victim);

        let events = world.advance(&[]);

        assert_eq!(world.victor(), Some(winner));
        assert_destroyed(&events, victim, loser);
        assert_eq!(world.seats[0].deaths, 0);
        assert_eq!(world.seats[1].deaths, 0);
    }
}

#[test]
fn a_courier_death_spends_no_hero_life_and_ends_nothing() {
    let mut world = mid_match();
    let courier = world.seats[0].courier.expect("Radiant courier");
    lethal_hit(&mut world, courier);

    world.advance(&[]);

    assert!(!world.alive(courier));
    assert_eq!(world.seats[0].deaths, 0);
    assert_eq!(world.victor(), None);
}

#[test]
fn simultaneous_second_hero_deaths_draw_in_either_hit_order() {
    for order in [[0, 1], [1, 0]] {
        let mut world = mid_match();
        for seat in &mut world.seats {
            seat.deaths = 1;
        }
        for seat in order {
            let victim = world.seats[seat].unit.expect("second life");
            lethal_hit(&mut world, victim);
        }

        world.advance(&[]);

        assert_eq!(world.victor(), Some(Team::Neutral));
        assert_eq!(world.seats[0].deaths, 2);
        assert_eq!(world.seats[1].deaths, 2);
    }
}

#[test]
fn simultaneous_tower_losses_draw_in_either_hit_order() {
    for order in [[Team::Radiant, Team::Dire], [Team::Dire, Team::Radiant]] {
        let mut world = mid_match();
        for team in order {
            let victim = tower(&world, team, rules::LANE_MID, 1);
            lethal_hit(&mut world, victim);
        }

        let events = world.advance(&[]);

        assert_eq!(world.victor(), Some(Team::Neutral));
        assert_eq!(
            events
                .iter()
                .filter(|event| { matches!(event.kind, EventKind::StructureDestroyed { .. }) })
                .count(),
            2
        );
    }
}

#[test]
fn a_tower_loss_and_an_opposing_second_hero_death_draw_in_either_order() {
    for tower_side in [Team::Radiant, Team::Dire] {
        for reverse in [false, true] {
            let mut world = mid_match();
            let seat = if tower_side == Team::Radiant { 1 } else { 0 };
            world.seats[seat].deaths = 1;
            let mut victims = [
                tower(&world, tower_side, rules::LANE_MID, 1),
                world.seats[seat].unit.expect("hero"),
            ];
            if reverse {
                victims.reverse();
            }
            for victim in victims {
                lethal_hit(&mut world, victim);
            }

            world.advance(&[]);

            assert_eq!(world.victor(), Some(Team::Neutral));
        }
    }
}

#[test]
fn simultaneous_first_hero_deaths_do_not_end_the_match() {
    let mut world = mid_match();
    for seat in 0..2 {
        let victim = world.seats[seat].unit.expect("first life");
        lethal_hit(&mut world, victim);
    }

    world.advance(&[]);

    assert_eq!(world.victor(), None);
    assert_eq!(world.seats[0].deaths, 1);
    assert_eq!(world.seats[1].deaths, 1);
}

#[test]
fn the_time_limit_draws_after_fifteen_minutes_of_play_and_the_pregame() {
    let mut world = mid_match();
    world.tick = TICK_CAP - 2;

    world.advance(&[]);

    assert_eq!(world.tick, TICK_CAP - 1);
    assert_eq!(world.victor(), None);
    world.advance(&[]);
    assert_eq!(
        world.tick,
        rules::PREGAME_TICKS + 15 * 60 * rules::TICKS_PER_SECOND
    );
    assert_eq!(world.victor(), Some(Team::Neutral));
    assert_eq!(world.match_stats().duration, TICK_CAP);
}

#[test]
fn play_goes_on_past_ten_minutes() {
    let mut world = mid_match();
    world.tick = 18_899;

    world.advance(&[]);
    world.advance(&[]);

    assert_eq!(world.tick, 18_901);
    assert_eq!(world.victor(), None);
}

#[test]
fn the_last_tick_completes_orders_gold_waves_and_a_tower_death_before_drawing() {
    let mut world = mid_match();
    world.tick = TICK_CAP - 1;
    let hero = world.seats[0].unit.expect("Radiant buyer");
    let boots = ItemId(crate::game::ITEM_BOOTS);
    let cost = crate::game::item_def(boots).expect("boots in catalog").cost;
    let gold = world.seats[0].gold;
    let order = Order::Buy { item: boots };
    assert_eq!(world.validate_order(SlotId(0), None, &order), Ok(()));
    let victim = tower(&world, Team::Dire, rules::LANE_MID, 1);
    lethal_hit(&mut world, victim);
    let command = Command {
        slot: SlotId(0),
        unit: None,
        order,
    };

    let events = world.advance(&[command]);

    assert!(
        world
            .inventory
            .get(hero)
            .expect("hero's bag")
            .held()
            .any(|stack| stack.id == boots)
    );
    assert_eq!(world.seats[0].gold, gold - cost + 1);
    let plan = wave_plan(FINAL_WAVE);
    assert_eq!(
        lane_creeps(&world),
        2 * (plan.melee + plan.ranged + plan.siege) as usize
    );
    assert_destroyed(&events, victim, Team::Dire);
    assert_eq!(world.victor(), Some(Team::Neutral));
    assert_eq!(world.match_stats().duration, TICK_CAP);
}

#[test]
fn a_world_already_at_the_limit_draws_without_a_late_step() {
    let mut world = mid_match();
    world.tick = TICK_CAP;
    let before = world.view_full();

    let events = world.step();

    assert_eq!(world.tick, TICK_CAP);
    assert!(events.is_empty());
    assert_eq!(world.view_full(), before);
    assert_eq!(world.victor(), Some(Team::Neutral));
}

#[test]
fn a_world_already_at_the_limit_ignores_late_commands() {
    let mut world = mid_match();
    world.tick = TICK_CAP;
    let before = world.view_full();
    let command = Command {
        slot: SlotId(0),
        unit: None,
        order: Order::Buy { item: ItemId(0) },
    };

    let events = world.advance(&[command]);

    assert_eq!(world.tick, TICK_CAP);
    assert_eq!(world.view_full(), before);
    assert!(events.is_empty());
    assert_eq!(world.match_stats().duration, TICK_CAP);
    assert_eq!(world.victor(), Some(Team::Neutral));
}

#[test]
fn a_tower_lost_a_tick_before_the_limit_wins_but_on_it_draws() {
    for (tick, expected) in [(TICK_CAP - 2, Team::Dire), (TICK_CAP - 1, Team::Neutral)] {
        let mut world = mid_match();
        world.tick = tick;
        let victim = tower(&world, Team::Radiant, rules::LANE_MID, 1);
        lethal_hit(&mut world, victim);

        let events = world.advance(&[]);

        assert_eq!(world.victor(), Some(expected));
        assert_eq!(world.match_stats().duration, tick + 1);
        assert_destroyed(&events, victim, Team::Radiant);
    }
}

#[test]
fn a_second_hero_death_on_the_last_tick_is_a_draw_after_it_is_counted() {
    let mut world = mid_match();
    world.tick = TICK_CAP - 1;
    world.seats[0].deaths = 1;
    let victim = world.seats[0].unit.expect("second life");
    lethal_hit(&mut world, victim);

    world.advance(&[]);

    assert_eq!(world.seats[0].deaths, 2);
    assert_eq!(world.victor(), Some(Team::Neutral));
}

#[test]
fn a_decided_match_stands_still_and_ignores_shop_orders() {
    for ending in 0..3 {
        let mut world = match ending {
            0 => {
                let mut world = mid_match();
                world.tick = TICK_CAP - 1;
                world
            }
            1 => {
                let mut world = mid_match();
                let victim = tower(&world, Team::Dire, rules::LANE_MID, 1);
                lethal_hit(&mut world, victim);
                world
            }
            _ => {
                // The standard game, decided at its Ancient.
                let mut world = match_world(MapId(0), STANDARD_RULES);
                let ancient = world
                    .entities
                    .iter()
                    .find(|&entity| {
                        world.kind.get(entity) == Some(&UnitKind::Ancient)
                            && world.team.get(entity) == Some(&Team::Dire)
                    })
                    .expect("the Dire Ancient");
                world.bury(vec![(ancient, None)], &mut Vec::new());
                world
            }
        };
        world.advance(&[]);
        assert!(world.victor().is_some(), "ending {ending}");
        let before = world.hash();
        let view = world.view_full();
        let stats = world.match_stats();
        let command = Command {
            slot: SlotId(0),
            unit: None,
            order: Order::Buy { item: ItemId(0) },
        };

        let events = world.advance(&[command]);
        let stepped = world.step();

        assert!(events.is_empty());
        assert!(stepped.is_empty());
        assert_eq!(world.hash(), before);
        assert_eq!(world.view_full(), view);
        assert_eq!(world.match_stats(), stats);
    }
}

#[test]
fn the_time_limit_does_not_depend_on_the_wall_clock_rate_or_mode() {
    for tick_rate in [1, 30, 120] {
        for mode in [TickMode::Lockstep, TickMode::Realtime] {
            let mut config = config(MapId(0), MID_RULES);
            config.tick_rate = tick_rate;
            config.mode = mode;
            let mut world = World::for_match(&config, config.rng());
            world.tick = TICK_CAP - 1;

            world.advance(&[]);

            assert_eq!(world.victor(), Some(Team::Neutral));
            assert_eq!(world.match_stats().duration, TICK_CAP);
        }
    }
}

#[test]
fn the_standard_rules_send_every_lane_the_map_has() {
    for (map, count) in [(MapId(0), 3), (MapId(1), 1)] {
        let mut world = World::on_map(map_of(map));
        world.tick = rules::FIRST_WAVE_TICK;
        let plan = wave_plan(1);

        world.spawn_waves();

        assert_eq!(
            lane_creeps(&world),
            2 * count * (plan.melee + plan.ranged + plan.siege) as usize
        );
    }
}

#[test]
fn the_standard_rules_have_no_time_limit() {
    for map in [MapId(0), MapId(1)] {
        let mut world = match_world(map, STANDARD_RULES);
        world.tick = TICK_CAP - 1;

        world.advance(&[]);
        world.advance(&[]);

        assert_eq!(world.victor(), None);
        assert_eq!(world.tick, TICK_CAP + 1);
    }
}

#[test]
fn the_standard_rules_are_not_ended_by_a_tower_or_by_hero_deaths() {
    for map in [MapId(0), MapId(1)] {
        for hero_loss in [false, true] {
            let mut world = match_world(map, STANDARD_RULES);
            let victim = if hero_loss {
                world.seats[0].deaths = 1;
                world.seats[0].unit.expect("second life")
            } else {
                tower(&world, Team::Radiant, rules::LANE_MID, 1)
            };
            lethal_hit(&mut world, victim);

            world.advance(&[]);

            assert_eq!(world.victor(), None);
        }
    }
}

#[test]
fn step_and_advance_share_the_ending_its_events_stats_and_hash() {
    for loser in [Some(Team::Radiant), Some(Team::Dire), None] {
        let mut direct = mid_match();
        let mut adapter = mid_match();
        for world in [&mut direct, &mut adapter] {
            if let Some(team) = loser {
                let victim = tower(world, team, rules::LANE_MID, 1);
                lethal_hit(world, victim);
            } else {
                world.tick = TICK_CAP - 1;
            }
        }

        let direct_events = direct.step();
        let adapter_events = adapter.advance(&[]);

        assert!(direct.victor().is_some());
        assert_eq!(direct.victor(), adapter.victor());
        assert_eq!(direct.match_stats(), adapter.match_stats());
        assert_eq!(direct.hash(), adapter.hash());
        assert_eq!(direct.view_full(), adapter.view_full());
        assert_eq!(direct_events.len(), adapter_events.len());
        for (direct, adapter) in direct_events.iter().zip(&adapter_events) {
            assert_eq!(direct.kind, adapter.kind);
            assert_eq!(direct.visible_to, adapter.visible_to);
        }
    }
}

#[test]
fn the_ending_crosses_the_wire_and_the_replay_whole() {
    for winner in [Team::Radiant, Team::Dire, Team::Neutral] {
        let mut world = mid_match();
        if winner == Team::Neutral {
            world.tick = TICK_CAP - 1;
        } else {
            let loser = if winner == Team::Radiant {
                Team::Dire
            } else {
                Team::Radiant
            };
            let victim = tower(&world, loser, rules::LANE_MID, 1);
            lethal_hit(&mut world, victim);
        }
        let events = world.advance(&[]);
        assert_eq!(world.victor(), Some(winner));
        let info = config(MapId(0), MID_RULES).info();
        assert_eq!(info.rules, MID_RULES, "the start of a match says its rules");
        let messages = [
            FromGame::MatchStart { info },
            FromGame::Snapshot {
                view: world.view_full(),
            },
            FromGame::Events {
                tick: world.tick,
                events: events.into_iter().map(|event| event.kind).collect(),
            },
            FromGame::MatchOver {
                winner: world.victor().expect("completed tick"),
                stats: world.match_stats(),
            },
        ];

        for message in messages {
            let frame = encode_frame_to_vec(&message).expect("native frame encodes");
            let decoded = decode_payload::<FromGame>(&frame[4..]).expect("native frame decodes");
            assert_eq!(decoded, message);
            let record = ReplayRecord::Msg(message);
            let frame = encode_frame_to_vec(&record).expect("replay frame encodes");
            assert_eq!(
                decode_payload::<ReplayRecord>(&frame[4..]).expect("replay frame decodes"),
                record
            );
        }
    }
}
