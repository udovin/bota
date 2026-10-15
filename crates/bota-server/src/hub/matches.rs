//! A match as the hub holds it, from creation to the archive.

use std::sync::mpsc::Sender;
use std::time::Instant;

use bota_proto::{
    AccountId, MatchId, MatchState, MatchStats, MatchSummary, ModeId, PlayerId, SlotId, Stage,
    Team, TournamentRef,
};

use crate::hub::{Mode, Roster};
use crate::run::MatchEvent;

/// One match, at whatever stage.
pub struct Match {
    /// Which match.
    pub id: MatchId,
    /// What the hall calls it.
    pub name: String,
    /// Which mode it is played under.
    pub mode: ModeId,
    /// The account that may kick from it. Absent for a tournament match.
    pub host: Option<AccountId>,
    /// The seed of its randomness.
    pub seed: u64,
    /// Where it is in its life.
    pub stage: Stage,
    /// Its seats.
    pub roster: Roster,
    /// Connections in it without a seat, and their display names.
    pub watchers: Vec<(PlayerId, String)>,
    /// The tournament game it is.
    pub tournament: Option<TournamentRef>,
    /// When it was created, in seconds since the Unix epoch.
    pub created_at: u64,
    /// When its stage last changed, or it was last left empty.
    pub since: Instant,
    /// Tournament only: when a team that has not shown up forfeits.
    pub deadline: Option<Instant>,
    /// The running thread's inbox, while it plays.
    pub run: Option<Sender<MatchEvent>>,
    /// Which side won. Absent until it ends with a winner.
    pub winner: Option<Team>,
    /// The final numbers, once it ended.
    pub stats: Option<MatchStats>,
    /// How many ticks it ran.
    pub duration: u32,
}

impl Match {
    /// A match gathering, with every seat open.
    pub fn new(
        id: MatchId,
        name: String,
        mode: &Mode,
        host: Option<AccountId>,
        seed: u64,
        created_at: u64,
    ) -> Match {
        Match {
            id,
            name,
            mode: mode.id,
            host,
            seed,
            stage: Stage::Gathering,
            roster: Roster::new(mode.seats),
            watchers: Vec::new(),
            tournament: None,
            created_at,
            since: Instant::now(),
            deadline: None,
            run: None,
            winner: None,
            stats: None,
            duration: 0,
        }
    }

    /// Whether a connection is in the match, seated or watching.
    pub fn holds(&self, player: PlayerId) -> bool {
        self.roster.seat_of_player(player).is_some() || self.watchers.iter().any(|w| w.0 == player)
    }

    /// Every connection in the match.
    pub fn players(&self) -> Vec<PlayerId> {
        self.roster
            .seats
            .iter()
            .filter_map(|s| s.player)
            .chain(self.watchers.iter().map(|w| w.0))
            .collect()
    }

    /// Whether anybody is connected to it.
    pub fn anyone_here(&self) -> bool {
        self.roster.anyone_present() || !self.watchers.is_empty()
    }

    /// The match as one connection sees it.
    pub fn state_for(&self, player: PlayerId) -> MatchState {
        MatchState {
            id: self.id,
            name: self.name.clone(),
            mode: self.mode,
            host: self.host,
            stage: self.stage,
            slots: self.roster.lobby_slots(),
            watchers: self.watchers.iter().map(|w| w.1.clone()).collect(),
            you: self.roster.seat_of_player(player).map(|s| s.slot),
        }
    }

    /// The match as the hall and the archive list it.
    pub fn summary(&self, map: bota_proto::MapId) -> MatchSummary {
        MatchSummary {
            id: self.id,
            name: self.name.clone(),
            mode: self.mode,
            map,
            stage: self.stage,
            host: self.host,
            seats: self.roster.summaries(),
            tournament: self.tournament,
            created_at: self.created_at,
            duration: self.duration,
            winner: self.winner,
            seed: match self.stage {
                Stage::Over | Stage::Abandoned => Some(self.seed),
                Stage::Gathering | Stage::Playing => None,
            },
        }
    }

    /// The seat a connection holds.
    pub fn seat_of_player(&self, player: PlayerId) -> Option<SlotId> {
        self.roster.seat_of_player(player).map(|s| s.slot)
    }
}
