//! Everything a match is created from.

use bota_proto::{MapId, MatchInfo, Pick, TickMode};

use crate::sim::MatchRng;

/// Full server-side description of one match.
///
/// [`MatchConfig::info`] is the part a participant may see; the rest never
/// leaves the server.
#[derive(Clone, Debug)]
pub struct MatchConfig {
    /// Identifies this match in logs and replay files.
    pub match_id: u64,
    /// Server secret the match randomness is derived from.
    pub master_key: [u8; 32],
    /// Every seat and its hero, sorted by slot.
    pub picks: Vec<Pick>,
    /// Which map is being played.
    pub map: MapId,
    /// Simulation ticks per second of wall-clock time.
    pub tick_rate: u16,
    /// How the server advances ticks.
    pub mode: TickMode,
    /// In lockstep, how long to wait for an acknowledgement before advancing
    /// with an empty order, in ticks of wall-clock time at `tick_rate`.
    pub ack_timeout_ticks: u32,
}

impl MatchConfig {
    /// The hidden randomness of this match.
    pub fn rng(&self) -> MatchRng {
        MatchRng::new(&self.master_key, self.match_id)
    }

    /// The projection onto the wire. Carries no secret.
    pub fn info(&self) -> MatchInfo {
        MatchInfo {
            match_id: self.match_id,
            map: self.map,
            tick_rate: self.tick_rate,
            mode: self.mode,
            picks: self.picks.clone(),
        }
    }
}
