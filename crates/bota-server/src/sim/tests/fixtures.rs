//! Building blocks shared by the simulation tests.

use bota_proto::{HeroId, MapId, Pick, SlotId, Team, TickMode};

use crate::sim::{CreepAi, LaneCreepAi, MatchConfig, World};

/// A 1v1 match configuration with a fixed key.
pub fn config() -> MatchConfig {
    MatchConfig {
        match_id: 7,
        master_key: [42u8; 32],
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
        map: MapId(0),
        tick_rate: 30,
        mode: TickMode::Lockstep,
        ack_timeout_ticks: 300,
    }
}

/// A fresh 1v1 world on the Dota map.
pub fn world() -> World {
    let cfg = config();
    let rng = cfg.rng();
    World::new(&cfg, rng)
}

/// A fresh 1v1 world on the small test map: one straight lane, open ground,
/// no forest. Builds in a fraction of the time the real map takes.
pub fn mini_world() -> World {
    let cfg = MatchConfig {
        map: MapId(1),
        ..config()
    };
    let rng = cfg.rng();
    World::new(&cfg, rng)
}

/// The hero unit id of a seat, which must be alive.
pub fn hero_id(world: &World, slot: u8) -> bota_proto::EntityId {
    world
        .seat(SlotId(slot))
        .expect("seat exists")
        .unit
        .expect("hero is alive")
}

/// Puts a lane creep that many waypoints along its route.
/// The Dota map, for tests that need landmarks without a world.
pub fn dota_map() -> &'static crate::sim::MapDef {
    crate::sim::map_of(bota_proto::MapId(0))
}

/// The small test map: one straight lane on open ground.
pub fn mini_map() -> &'static crate::sim::MapDef {
    crate::sim::map_of(bota_proto::MapId(1))
}

pub fn set_lane_step(world: &mut World, id: bota_proto::EntityId, step: u16) {
    let unit = world.units.get_mut(id).expect("a live creep");
    unit.ai = Some(CreepAi::Lane(LaneCreepAi {
        step,
        ..LaneCreepAi::new()
    }));
}

/// Points a lane creep at the route waypoint nearest a spot, so it marches on
/// from there rather than back to the start of its lane.
pub fn aim_along_lane(world: &mut World, id: bota_proto::EntityId, lane: u8) {
    let unit = world.units.get(id).expect("a live creep");
    let (team, at) = (unit.team, unit.pos);
    let route =
        &crate::sim::lane_routes(world.map)[crate::sim::team_index(team)][usize::from(lane)];
    // The waypoint after the nearest one, so the creep marches on rather than
    // doubling back to a point it has already passed.
    let nearest = route
        .iter()
        .enumerate()
        .min_by_key(|(_, w)| at.distance_squared(**w))
        .map(|(i, _)| i)
        .unwrap_or(0);
    let step = nearest.saturating_add(1).min(route.len() - 1) as u16;
    set_lane_step(world, id, step);
}
