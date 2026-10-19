//! A match as the hub holds it, from creation to the archive.

use std::fs::File;
use std::time::Instant;

use bota_proto::{
    AccountId, MatchId, MatchState, MatchStats, MatchSummary, ModeId, Pass, PlayerId, SlotId,
    Stage, Team, TournamentRef,
};

use crate::hub::{Mode, Roster};

/// One match, at whatever stage.
pub struct Match {
    /// Which match.
    pub id: MatchId,
    /// What the hall calls it.
    pub name: String,
    /// Which mode it is played under.
    pub mode: ModeId,
    /// The account that may start it and kick from it. Absent for a match
    /// that starts on its own: a tournament's, or the queue's.
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
    /// Whether the matchmaking queue made it. Its seats are its accounts'
    /// from the start, and it starts as it is made.
    pub matchmade: bool,
    /// When it was created, in seconds since the Unix epoch.
    pub created_at: u64,
    /// When its stage last changed, or it was last left empty.
    pub since: Instant,
    /// While a tournament game gathers: when a team that has not shown up
    /// forfeits it.
    pub deadline: Option<Instant>,
    /// The link of the game server it is on, from its start to its end.
    pub server: Option<PlayerId>,
    /// Everybody let into it on its game server, from its start to its end.
    pub passes: Vec<Pass>,
    /// Its replay file as its game server sends it, from its start to its
    /// end.
    pub replay: Option<File>,
    /// Whether it was started and waits for a game server with room.
    pub waiting_server: bool,
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
            matchmade: false,
            created_at,
            since: Instant::now(),
            deadline: None,
            server: None,
            passes: Vec::new(),
            replay: None,
            waiting_server: false,
            winner: None,
            stats: None,
            duration: 0,
        }
    }

    /// Whether a connection is in the match, seated or watching.
    pub fn holds(&self, player: PlayerId) -> bool {
        !self.roster.sat_on_by(player).is_empty() || self.watchers.iter().any(|w| w.0 == player)
    }

    /// Every connection in the match, each once.
    pub fn players(&self) -> Vec<PlayerId> {
        let mut players: Vec<PlayerId> = self
            .roster
            .seats
            .iter()
            .filter_map(|s| s.player)
            .chain(self.watchers.iter().map(|w| w.0))
            .collect();
        players.sort_unstable();
        players.dedup();
        players
    }

    /// Whether anybody is connected to it.
    pub fn anyone_here(&self) -> bool {
        self.roster.anyone_present() || !self.watchers.is_empty()
    }

    /// Whether its seats belong to the accounts they were given to, a
    /// tournament's or the queue's, rather than to whoever takes them.
    pub fn seats_fixed(&self) -> bool {
        self.tournament.is_some() || self.matchmade
    }

    /// Whether it is on its game server, its heroes being picked or it
    /// being played.
    pub fn started(&self) -> bool {
        matches!(self.stage, Stage::Picking | Stage::Playing)
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
            yours: self.roster.sat_on_by(player),
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
                Stage::Gathering | Stage::Picking | Stage::Playing => None,
            },
        }
    }

    /// The seats a connection sits on, in slot order.
    pub fn seats_of_player(&self, player: PlayerId) -> Vec<SlotId> {
        self.roster.sat_on_by(player)
    }

    /// The side a connection's seats are on. Absent for a spectator.
    pub fn side_of_player(&self, player: PlayerId) -> Option<Team> {
        let slot = *self.roster.sat_on_by(player).first()?;
        self.roster.at(slot).map(|seat| seat.team)
    }
}
