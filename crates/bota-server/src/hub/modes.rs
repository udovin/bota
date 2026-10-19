//! The preset rule sets a match is played under.

use bota_proto::{MapId, ModeId, ModeInfo, Rules, TickMode};

use crate::game::{MID_RULES, STANDARD_RULES};

/// One preset: everything about a match that is not who plays it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mode {
    /// Which mode.
    pub id: ModeId,
    /// What the hall calls it.
    pub name: String,
    /// Which map it is played on.
    pub map: MapId,
    /// Seats in a match, half per side.
    pub seats: u8,
    /// Most seats one account may hold in a match of it, all on one side.
    pub seats_per_player: u8,
    /// How the server advances ticks.
    pub tick: TickMode,
    /// Simulation ticks per second.
    pub tick_rate: u16,
    /// Lockstep: tick-lengths to wait for a straggler before moving on.
    pub ack_timeout_ticks: u32,
    /// What its matches are played to and what runs in them.
    pub rules: Rules,
    /// Whether spectators may write in the chat of a match.
    pub spectator_chat: bool,
}

impl Mode {
    /// Whether one account may hold this many seats in a match of it: one
    /// or more, within its limit and one side.
    pub fn admits(&self, seats: u8) -> bool {
        seats >= 1 && seats <= self.seats_per_player && seats <= self.seats / 2
    }

    /// The projection onto the wire.
    pub fn info(&self) -> ModeInfo {
        ModeInfo {
            id: self.id,
            name: self.name.clone(),
            map: self.map,
            seats: self.seats,
            seats_per_player: self.seats_per_player,
            tick: self.tick,
            tick_rate: self.tick_rate,
            rules: self.rules,
            spectator_chat: self.spectator_chat,
        }
    }
}

/// The modes a server offers when nothing else is asked for.
pub fn standard_modes() -> Vec<Mode> {
    let mode = |id: u16, name: &str, map: u16, seats: u8, tick: TickMode, rules: Rules| Mode {
        id: ModeId(id),
        name: name.to_string(),
        map: MapId(map),
        seats,
        seats_per_player: 1,
        tick,
        tick_rate: 30,
        ack_timeout_ticks: 150,
        rules,
        spectator_chat: false,
    };
    let sandbox = Rules {
        cheats: true,
        ..STANDARD_RULES
    };
    // An arena lets one account drive a whole side.
    let arena = |id: u16, name: &str, seats: u8| Mode {
        seats_per_player: seats / 2,
        ..mode(id, name, 0, seats, TickMode::Lockstep, STANDARD_RULES)
    };
    vec![
        mode(0, "Dota 5v5", 0, 10, TickMode::Realtime, STANDARD_RULES),
        mode(1, "Dota 1v1", 0, 2, TickMode::Realtime, STANDARD_RULES),
        mode(2, "Mid 1v1", 0, 2, TickMode::Realtime, MID_RULES),
        mode(3, "Demo", 1, 2, TickMode::Realtime, sandbox),
        mode(4, "Bot arena 1v1", 0, 2, TickMode::Lockstep, STANDARD_RULES),
        mode(5, "Bot arena mid", 0, 2, TickMode::Lockstep, MID_RULES),
        mode(6, "Dota 2v2", 0, 4, TickMode::Realtime, STANDARD_RULES),
        arena(7, "Bot arena 2v2", 4),
        arena(8, "Bot arena 5v5", 10),
    ]
}
