//! What the hub and a game server say to each other: who proves the shared
//! key, which matches run where, who may enter them, and how they ended.

use serde::{Deserialize, Serialize};

use crate::{
    MapId, MatchId, MatchStats, Nonce, Pick, Proof, Rules, SlotId, Team, TickMode, Ticket,
};

/// Who may enter a running match, and on which seats.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Pass {
    /// What the connection shows to be let in.
    pub ticket: Ticket,
    /// The login its lines are signed with and its cheats told under.
    pub login: String,
    /// The seats it drives, in slot order, all on one side. Empty for a
    /// spectator.
    pub seats: Vec<SlotId>,
}

/// One seat of a match a game server is given.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SeatSetup {
    /// Which seat.
    pub slot: SlotId,
    /// Which side it plays for.
    pub team: Team,
    /// The login of the account holding it.
    pub login: String,
}

/// Everything a game server needs to run one match.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MatchSetup {
    /// Which match.
    pub match_id: MatchId,
    /// What the match's randomness is drawn from.
    pub seed: u64,
    /// Which map it is played on.
    pub map: MapId,
    /// Every seat, sorted by slot. Their heroes are picked on the game
    /// server.
    pub seats: Vec<SeatSetup>,
    /// Milliseconds the seats have to pick their heroes before every seat
    /// without one is given one at random.
    pub pick_ms: u32,
    /// What it is played to and what runs in it.
    pub rules: Rules,
    /// How the game server advances ticks.
    pub tick: TickMode,
    /// Simulation ticks per second of wall-clock time.
    pub tick_rate: u16,
    /// Lockstep: tick-lengths to wait for a straggler before moving on.
    pub ack_timeout_ticks: u32,
    /// Milliseconds every seat may be away before the match is abandoned.
    pub abandon_after_ms: u32,
    /// Whether spectators may write in the match's chat.
    pub spectator_chat: bool,
    /// Everybody let in from the start.
    pub passes: Vec<Pass>,
}

/// Anything the hub can say to a game server.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Assign {
    /// First message on the link: a number for the game server to prove the
    /// shared key over.
    Challenge {
        /// The number.
        nonce: Nonce,
    },
    /// The game server's proof held, and the link is open: the hub's own
    /// proof over the number the game server sent.
    Accepted {
        /// The proof.
        proof: Proof,
    },
    /// Run a match.
    Start {
        /// Everything about it.
        setup: MatchSetup,
    },
    /// Let one more connection into a running match: a spectator, or a seat
    /// holder coming back.
    Admit {
        /// Which match.
        match_id: MatchId,
        /// Who, and on which seats.
        pass: Pass,
    },
    /// Seats of a running match were abandoned: nobody may drive them again,
    /// and a side whose every seat is abandoned loses.
    Abandon {
        /// Which match.
        match_id: MatchId,
        /// The seats given up, in slot order.
        seats: Vec<SlotId>,
        /// The login that held them, for the match to be told.
        login: String,
    },
    /// End a match without a winner.
    Stop {
        /// Which match.
        match_id: MatchId,
    },
}

/// Anything a game server can say to the hub.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Report {
    /// Answers an [`Assign::Challenge`]: the game server's proof, a number
    /// for the hub to prove itself over, where players reach it, and how
    /// many matches it runs at once.
    Register {
        /// The proof over the hub's number.
        proof: Proof,
        /// The number for the hub.
        nonce: Nonce,
        /// Where players connect to, as `host:port`.
        addr: String,
        /// Matches it runs at once.
        capacity: u16,
    },
    /// Every seat of a match has its hero, and the match is played.
    Picked {
        /// Which match.
        match_id: MatchId,
        /// Every seat and its hero, sorted by slot.
        picks: Vec<Pick>,
    },
    /// The next piece of a running match's replay file, in file order.
    Replay {
        /// Which match.
        match_id: MatchId,
        /// The bytes.
        bytes: Vec<u8>,
    },
    /// A match ended.
    Ended {
        /// Which match.
        match_id: MatchId,
        /// Which side won, [`Team::Neutral`] for a draw. Absent when it was
        /// abandoned or stopped.
        winner: Option<Team>,
        /// The final numbers.
        stats: MatchStats,
        /// How many ticks it ran.
        duration: u32,
    },
}
