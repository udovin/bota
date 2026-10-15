//! The preset rule sets a match is played under.

use bota_proto::{MapId, ModeId, ModeInfo, TickMode};

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
    /// How the server advances ticks.
    pub tick: TickMode,
    /// Simulation ticks per second.
    pub tick_rate: u16,
    /// Lockstep: tick-lengths to wait for a straggler before moving on.
    pub ack_timeout_ticks: u32,
    /// Whether cheat orders are honoured.
    pub cheats: bool,
}

impl Mode {
    /// The projection onto the wire.
    pub fn info(&self) -> ModeInfo {
        ModeInfo {
            id: self.id,
            name: self.name.clone(),
            map: self.map,
            seats: self.seats,
            tick: self.tick,
            tick_rate: self.tick_rate,
            cheats: self.cheats,
        }
    }
}

/// The modes a server offers when nothing else is asked for.
pub fn standard_modes() -> Vec<Mode> {
    let mode = |id: u16, name: &str, map: u16, seats: u8, tick: TickMode, cheats: bool| Mode {
        id: ModeId(id),
        name: name.to_string(),
        map: MapId(map),
        seats,
        tick,
        tick_rate: 30,
        ack_timeout_ticks: 150,
        cheats,
    };
    vec![
        mode(0, "Dota 5v5", 0, 10, TickMode::Realtime, false),
        mode(1, "Dota 1v1", 0, 2, TickMode::Realtime, false),
        mode(2, "Mid 1v1", 2, 2, TickMode::Realtime, false),
        mode(3, "Demo", 1, 2, TickMode::Realtime, true),
        mode(4, "Bot arena 1v1", 0, 2, TickMode::Lockstep, false),
        mode(5, "Bot arena mid", 2, 2, TickMode::Lockstep, false),
        mode(6, "Dota 2v2", 0, 4, TickMode::Realtime, false),
    ]
}
