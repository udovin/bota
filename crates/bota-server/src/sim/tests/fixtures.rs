//! Building blocks shared by the simulation tests.

use bota_proto::{HeroId, MapId, Pick, SlotId, Team, TickMode};

use crate::sim::{MatchConfig, World};

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

/// A fresh 1v1 world.
pub fn world() -> World {
    let cfg = config();
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
