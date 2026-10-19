//! The matchmaking queue: who waits for a match of which mode, and the
//! matches found for them that wait to be accepted.

use std::time::Instant;

use bota_proto::{AccountId, MatchId, ModeId, PlayerId, Team};

/// One connection waiting in the queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Searcher {
    /// Which connection.
    pub player: PlayerId,
    /// The account it is signed in as.
    pub account: AccountId,
    /// The mode it waits for.
    pub mode: ModeId,
    /// How many seats it takes, all on one side.
    pub seats: u8,
    /// Its place in the queue: a smaller number came first, and keeps its
    /// place when a match found for it falls through.
    pub turn: u64,
}

/// One connection a found match is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    /// Who, as it waited in the queue.
    pub searcher: Searcher,
    /// The side it will play on.
    pub side: Team,
    /// Whether it has accepted.
    pub accepted: bool,
}

/// A match found in the queue, waiting for everybody it is for to accept.
#[derive(Clone, Debug)]
pub struct Found {
    /// The id the match will have.
    pub id: MatchId,
    /// Which mode.
    pub mode: ModeId,
    /// Everybody it is for, Radiant first.
    pub members: Vec<Member>,
    /// When whoever has not accepted is dropped from the queue.
    pub deadline: Instant,
}

impl Found {
    /// How many members have accepted.
    pub fn accepted(&self) -> usize {
        self.members.iter().filter(|m| m.accepted).count()
    }

    /// Whether a connection is one of the members.
    pub fn has(&self, player: PlayerId) -> bool {
        self.members.iter().any(|m| m.searcher.player == player)
    }
}

/// Fills both sides of a match of one mode from the queue, in queue order:
/// each searcher goes to the first side with room for all its seats, and one
/// with room on neither waits for a later match. Returns where each chosen
/// searcher stands in the queue and its side, or nothing while the queue
/// cannot fill both sides of `side` seats.
pub fn fill_sides(queue: &[Searcher], mode: ModeId, side: u8) -> Option<Vec<(usize, Team)>> {
    if side == 0 {
        return None;
    }
    let mut order: Vec<usize> = (0..queue.len())
        .filter(|&at| queue[at].mode == mode)
        .collect();
    order.sort_by_key(|&at| queue[at].turn);
    let (mut radiant, mut dire) = (0u8, 0u8);
    let mut chosen = Vec::new();
    for at in order {
        let seats = queue[at].seats;
        if radiant + seats <= side {
            radiant += seats;
            chosen.push((at, Team::Radiant));
        } else if dire + seats <= side {
            dire += seats;
            chosen.push((at, Team::Dire));
        }
        if radiant == side && dire == side {
            return Some(chosen);
        }
    }
    None
}
