//! Streaming a stored replay to a connection.

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};
use std::thread;

use bota_proto::{FromHub, MatchId, encode_frame_to_vec};

use crate::net::Outbox;

/// Bytes of replay in one [`FromHub::ReplayChunk`].
pub const REPLAY_CHUNK: usize = 64 * 1024;

/// Chunks allowed to wait in an outbox before the next one is read.
const CHUNKS_IN_FLIGHT: usize = 8;

/// One replay to send to one connection.
pub struct FetchJob {
    /// Which match.
    pub id: MatchId,
    /// The file to send.
    pub path: PathBuf,
    /// Where to send it.
    pub outbox: Arc<Outbox>,
}

/// The thread that sends replays, one job at a time.
pub struct Fetcher {
    jobs: Sender<FetchJob>,
}

impl Fetcher {
    /// Starts the thread.
    pub fn spawn() -> Fetcher {
        let (jobs, rx) = channel::<FetchJob>();
        thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                send_file(&job);
            }
        });
        Fetcher { jobs }
    }

    /// Queues a replay to be sent.
    pub fn fetch(&self, job: FetchJob) {
        let _ = self.jobs.send(job);
    }
}

/// Reads the file in chunks and queues each one, waiting for the outbox to
/// drain between them. Ends with a [`FromHub::ReplayEnd`] carrying the
/// bytes sent, whether or not the file was read whole.
fn send_file(job: &FetchJob) {
    let mut offset = 0u64;
    if let Ok(mut file) = File::open(&job.path) {
        let mut buf = vec![0u8; REPLAY_CHUNK];
        loop {
            if !job.outbox.wait_below(CHUNKS_IN_FLIGHT) {
                return;
            }
            let read = match file.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let chunk = FromHub::ReplayChunk {
                match_id: job.id,
                offset,
                bytes: buf[..read].to_vec(),
            };
            if let Ok(frame) = encode_frame_to_vec(&chunk) {
                job.outbox.push(frame);
            }
            offset += read as u64;
        }
    }
    if let Ok(frame) = encode_frame_to_vec(&FromHub::ReplayEnd {
        match_id: job.id,
        total: offset,
    }) {
        job.outbox.push(frame);
    }
}
