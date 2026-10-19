//! The game servers a hub hands its matches to.

use bota_proto::{MatchId, Nonce};

use crate::net::Connection;

/// One game server's link to the hub.
pub struct ServerLink {
    /// The socket.
    pub conn: Connection,
    /// The number the hub asked it to prove the shared key over.
    pub challenge: Nonce,
    /// Where players reach it, and how many matches it runs at once. Absent
    /// until it has proved the key.
    pub offer: Option<(String, u16)>,
    /// The matches it runs now.
    pub running: Vec<MatchId>,
}

impl ServerLink {
    /// Whether it has proved the key and has room for one more match.
    pub fn has_room(&self) -> bool {
        self.offer
            .as_ref()
            .is_some_and(|(_, capacity)| self.running.len() < usize::from(*capacity))
    }
}
