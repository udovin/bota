//! Everything the command line and the tests decide about a hub.

use std::path::PathBuf;
use std::time::Duration;

use crate::hub::{HashCost, Mode, standard_modes};

/// How a hub is set up.
#[derive(Clone, Debug)]
pub struct HubOpts {
    /// Where replays, accounts and tournaments are kept.
    pub data: PathBuf,
    /// The key the hub and its game servers share.
    pub key: [u8; 32],
    /// The modes on offer, in id order.
    pub modes: Vec<Mode>,
    /// Whether cheat commands are honoured in every mode, whatever its rules
    /// say.
    pub cheats: bool,
    /// How expensive password hashing is.
    pub hash_cost: HashCost,
    /// How long a token signs in for.
    pub token_life: Duration,
    /// How long every seat of a running match may be away before it is
    /// abandoned.
    pub abandon_after: Duration,
    /// How long a tournament pairing waits for its teams before the absent
    /// side forfeits the game.
    pub ready_timeout: Duration,
    /// How long a match found in the queue waits for everybody to accept it.
    pub accept_timeout: Duration,
    /// How long the seats of a match have to pick their heroes on its game
    /// server before every seat without one is given one at random.
    pub pick_timeout: Duration,
    /// How long a gathering match stays open with nobody in it.
    pub empty_ttl: Duration,
    /// How long an ended match stays in the hall with nobody in it.
    pub over_ttl: Duration,
    /// Least time between two hall broadcasts.
    pub hall_interval: Duration,
}

impl HubOpts {
    /// A hub as the command line runs it.
    pub fn standard(data: PathBuf, key: [u8; 32]) -> HubOpts {
        HubOpts {
            data,
            key,
            modes: standard_modes(),
            cheats: false,
            hash_cost: HashCost::Full,
            token_life: Duration::from_secs(7 * 24 * 3600),
            abandon_after: Duration::from_secs(120),
            ready_timeout: Duration::from_secs(300),
            accept_timeout: Duration::from_secs(15),
            pick_timeout: Duration::from_secs(60),
            empty_ttl: Duration::from_secs(600),
            over_ttl: Duration::from_secs(600),
            hall_interval: Duration::from_secs(1),
        }
    }
}
