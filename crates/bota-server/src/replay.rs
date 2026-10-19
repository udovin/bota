//! Writing a replay: the frames of a match, handed on in pieces.

use bota_proto::{ReplayRecord, encode_frame_to_vec};

/// Bytes gathered before a piece is handed on.
pub const REPLAY_PIECE: usize = 64 * 1024;

/// Gathers [`ReplayRecord`] frames and hands them on in pieces of about
/// [`REPLAY_PIECE`] bytes, in file order.
pub struct ReplayWriter {
    out: Option<Box<dyn FnMut(Vec<u8>) + Send>>,
    gathered: Vec<u8>,
}

impl ReplayWriter {
    /// A writer that hands every piece to `out`.
    pub fn to(out: Box<dyn FnMut(Vec<u8>) + Send>) -> ReplayWriter {
        ReplayWriter {
            out: Some(out),
            gathered: Vec::new(),
        }
    }

    /// A writer that records nothing.
    pub fn disabled() -> ReplayWriter {
        ReplayWriter {
            out: None,
            gathered: Vec::new(),
        }
    }

    /// Appends one record, handing a piece on once enough has gathered.
    pub fn record(&mut self, record: &ReplayRecord) {
        if self.out.is_none() {
            return;
        }
        if let Ok(frame) = encode_frame_to_vec(record) {
            self.gathered.extend_from_slice(&frame);
        }
        if self.gathered.len() >= REPLAY_PIECE {
            self.hand_on();
        }
    }

    /// Hands on whatever has gathered.
    pub fn finish(mut self) {
        self.hand_on();
    }

    fn hand_on(&mut self) {
        if self.gathered.is_empty() {
            return;
        }
        let piece = std::mem::take(&mut self.gathered);
        if let Some(out) = self.out.as_mut() {
            out(piece);
        }
    }
}
