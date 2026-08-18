//! Playing a `.brp` file as if a server were sending it.

use std::collections::VecDeque;
use std::path::Path;

use bota_proto::{FrameReader, ReplayRecord, ServerMsg};

/// Feeds recorded frames out on the recorded clock.
///
/// A snapshot is released once the playback clock reaches its tick; everything
/// between snapshots is released along with it. Order records are for future
/// overlays and are skipped for now.
pub struct ReplayPlayer {
    records: VecDeque<ReplayRecord>,
    /// Whether the clock is running.
    pub paused: bool,
    /// Clock multiplier: 1.0 is the recorded pace.
    pub speed: f32,
    clock_ticks: f64,
    tick_rate: f64,
}

impl ReplayPlayer {
    /// Loads a whole replay file.
    pub fn load(path: &Path) -> std::io::Result<ReplayPlayer> {
        let bytes = std::fs::read(path)?;
        let mut reader = FrameReader::new();
        reader.push(&bytes);
        let mut records = VecDeque::new();
        while let Ok(Some(record)) = reader.next_message::<ReplayRecord>() {
            records.push_back(record);
        }
        if records.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "no replay records in the file",
            ));
        }
        Ok(ReplayPlayer {
            records,
            paused: false,
            speed: 1.0,
            clock_ticks: 0.0,
            tick_rate: 30.0,
        })
    }

    /// Advances the clock and returns every message now due.
    pub fn poll(&mut self, dt: f32) -> Vec<ServerMsg> {
        if !self.paused {
            self.clock_ticks += f64::from(dt) * f64::from(self.speed) * self.tick_rate;
        }
        self.drain_due()
    }

    /// Jumps the clock forward by whole ticks, paused or not.
    pub fn advance_ticks(&mut self, ticks: f64) -> Vec<ServerMsg> {
        self.clock_ticks += ticks;
        self.drain_due()
    }

    /// Whether every record has been played.
    pub fn finished(&self) -> bool {
        self.records.is_empty()
    }

    fn drain_due(&mut self) -> Vec<ServerMsg> {
        let mut due = Vec::new();
        while let Some(record) = self.records.front() {
            match record {
                ReplayRecord::Orders { .. } => {
                    self.records.pop_front();
                }
                ReplayRecord::Msg(ServerMsg::Snapshot { view })
                    if f64::from(view.tick) > self.clock_ticks =>
                {
                    break;
                }
                ReplayRecord::Msg(_) => {
                    let ReplayRecord::Msg(msg) = self.records.pop_front().expect("peeked") else {
                        unreachable!("the front was just matched as Msg");
                    };
                    if let ServerMsg::MatchStart { info } = &msg {
                        self.tick_rate = f64::from(info.tick_rate.max(1));
                    }
                    due.push(msg);
                }
            }
        }
        due
    }
}
