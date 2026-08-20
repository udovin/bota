//! What a match looked like to the network, kept so it can be learned from.
//!
//! One frame a decision: the rows the network was shown and which of them was
//! taken. That is everything training needs and nothing else — a frame does
//! not know what the world was, only what the network saw of it, which is the
//! same thing the network will see when it plays.
//!
//! Frames are kept in memory for the length of a match and written out
//! afterwards. A match of eighteen thousand ticks decides on a few thousand of
//! them, and a decision is a couple of dozen rows of forty-eight numbers, so a
//! match is a few megabytes.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use crate::FEATURES;

/// One decision: what was shown, and what was taken.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// One row a candidate.
    pub rows: Vec<Vec<f32>>,
    /// Which row was taken.
    pub chosen: usize,
    /// What the match this came from was worth. Filled in afterwards, since
    /// nothing is known about it until the match ends.
    pub worth: f32,
}

/// Every decision of one seat's match.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recording {
    /// The frames, in the order they happened.
    pub frames: Vec<Frame>,
}

impl Recording {
    /// Nothing recorded yet.
    pub fn new() -> Recording {
        Recording::default()
    }

    /// Adds one decision.
    pub fn put(&mut self, rows: Vec<Vec<f32>>, chosen: usize) {
        self.frames.push(Frame {
            rows,
            chosen,
            worth: 0.0,
        });
    }

    /// Says what the whole match was worth, on every frame of it.
    ///
    /// Every decision of a match shares its outcome. That is a blunt way to
    /// hand out credit and it is the honest one here: nothing in the match
    /// says which tick won it.
    pub fn worth_was(&mut self, worth: f32) {
        for frame in &mut self.frames {
            frame.worth = worth;
        }
    }

    /// Writes the frames out.
    ///
    /// Written as they sit in memory, little-endian: a recording is scratch
    /// for a training run, not something to keep or send.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let mut out = BufWriter::new(File::create(path)?);
        out.write_all(&(self.frames.len() as u32).to_le_bytes())?;
        for frame in &self.frames {
            out.write_all(&(frame.rows.len() as u16).to_le_bytes())?;
            out.write_all(&(frame.chosen as u16).to_le_bytes())?;
            out.write_all(&frame.worth.to_le_bytes())?;
            for row in &frame.rows {
                for number in row {
                    out.write_all(&number.to_le_bytes())?;
                }
            }
        }
        out.flush()
    }

    /// Reads frames back, adding them to whatever is here already.
    pub fn read_from(&mut self, path: &Path) -> std::io::Result<()> {
        let mut file = BufReader::new(File::open(path)?);
        let mut four = [0u8; 4];
        let mut two = [0u8; 2];
        file.read_exact(&mut four)?;
        let count = u32::from_le_bytes(four);
        for _ in 0..count {
            file.read_exact(&mut two)?;
            let rows = usize::from(u16::from_le_bytes(two));
            file.read_exact(&mut two)?;
            let chosen = usize::from(u16::from_le_bytes(two));
            file.read_exact(&mut four)?;
            let worth = f32::from_le_bytes(four);
            let mut held = Vec::with_capacity(rows);
            for _ in 0..rows {
                let mut row = Vec::with_capacity(FEATURES);
                for _ in 0..FEATURES {
                    file.read_exact(&mut four)?;
                    row.push(f32::from_le_bytes(four));
                }
                held.push(row);
            }
            self.frames.push(Frame {
                rows: held,
                chosen,
                worth,
            });
        }
        Ok(())
    }

    /// How many decisions are held.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether none are.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}
