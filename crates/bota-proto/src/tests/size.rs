//! Budgets for how much a snapshot costs on the wire.
//!
//! Full views are sent every tick with no delta encoding, so the size of a
//! snapshot is the bandwidth of the game. These bounds are loose enough to
//! ignore ordinary changes and tight enough to catch a field that costs far
//! more than it looks.

use super::fixtures::*;
use crate::*;

fn encoded_len<T: serde::Serialize>(value: &T) -> usize {
    encode_frame_to_vec(value).expect("encode").len()
}

/// Units in a 1v1 match: two heroes, two creep waves, towers, fountains.
const UNITS_1V1: u32 = 25;

/// Ticks per second the server runs at.
const TICK_RATE: usize = 30;

#[test]
fn a_hero_stays_under_178_bytes() {
    let len = encoded_len(&hero_unit());
    assert!(len <= 178, "hero unit grew to {len} bytes");
}

#[test]
fn a_creep_stays_under_83_bytes() {
    let len = encoded_len(&creep_unit(1));
    assert!(len <= 83, "creep unit grew to {len} bytes");
}

#[test]
fn a_1v1_snapshot_fits_in_a_tcp_burst() {
    let msg = ServerMsg::Snapshot {
        view: world_view(UNITS_1V1 - 1),
    };
    let len = encoded_len(&msg);
    let per_second = len * TICK_RATE;

    assert!(
        len <= 4096,
        "snapshot of {UNITS_1V1} units grew to {len} bytes, \
         which is {} KB/s per viewer at {TICK_RATE} Hz",
        per_second / 1024,
    );
}

#[test]
fn an_order_stays_small() {
    for order in all_orders() {
        // Cheats may carry a whole modifier spec; ordinary orders may not
        // grow into that room.
        let budget = if matches!(order, Order::Cheat { .. }) {
            64
        } else {
            32
        };
        let msg = ClientMsg::Order {
            seq: 12345,
            unit: None,
            order,
        };
        let len = encoded_len(&msg);
        assert!(len <= budget, "order encoded to {len} bytes: {msg:?}");
    }
}

#[test]
fn a_modifier_cheat_encodes_canonically() {
    // Postcard: order tag 10, cheat tag 4, target tag 2, entity idx and
    // generation varints, seven zigzag varints, then the tick count. The
    // bytes are pinned so a field change cannot pass unnoticed.
    let order = Order::Cheat {
        cheat: Cheat::ApplyModifier {
            target: Target::Unit(entity(7)),
            spec: ModifierSpec {
                magic_resist: 1_000,
                status_resist: 2_000,
                physical_damage: 11_000,
                magic_damage: 12_000,
                pure_damage: 13_000,
                cooldown_rate: 9_000,
                mana_cost_rate: 8_000,
            },
            ticks: 900,
        },
    };
    let bytes = encode_frame_to_vec(&order).expect("encode");
    assert_eq!(
        bytes,
        vec![
            25, 0, 0, 0,  // frame length
            10, // Order::Cheat
            4,  // Cheat::ApplyModifier
            2, 7, 1, // Target::Unit(entity(7))
            0xD0, 0x0F, // magic_resist 1_000 zigzag
            0xA0, 0x1F, // status_resist 2_000 zigzag
            0xF0, 0xAB, 0x01, // physical_damage 11_000 zigzag
            0xC0, 0xBB, 0x01, // magic_damage 12_000 zigzag
            0x90, 0xCB, 0x01, // pure_damage 13_000 zigzag
            0xD0, 0x8C, 0x01, // cooldown_rate 9_000 zigzag
            0x80, 0x7D, // mana_cost_rate 8_000 zigzag
            0x84, 0x07, // ticks 900
        ]
    );
}

#[test]
fn an_empty_view_costs_almost_nothing() {
    let msg = ServerMsg::Snapshot {
        view: WorldView {
            tick: 0,
            viewer: None,
            units: Vec::new(),
            projectiles: Vec::new(),
            players: Vec::new(),
            felled_trees: Vec::new(),
            planted_trees: Vec::new(),
            loot: Vec::new(),
        },
    };
    let len = encoded_len(&msg);
    assert!(len <= 16, "an empty snapshot cost {len} bytes");
}
