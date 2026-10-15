//! The hall: accounts, matches before and after they run, and tournaments.

use crate::{
    AccountId, HeroId, MapId, MatchId, ModeId, SlotId, Team, TickMode, Token, TournamentId,
};
use serde::{Deserialize, Serialize};

/// How a connection says who it is, in its first message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Auth {
    /// Makes a new account and signs it in.
    Register {
        /// The name of the account, unique on the server: two to twenty-four
        /// ASCII letters, digits, dots, dashes or underscores.
        login: String,
        /// Its password, one to sixty-four characters.
        password: String,
    },
    /// Signs an existing account in.
    Login {
        /// The name of the account.
        login: String,
        /// Its password.
        password: String,
    },
    /// Signs in again with a token from an earlier [`ServerMsg::Welcome`].
    ///
    /// [`ServerMsg::Welcome`]: crate::ServerMsg::Welcome
    Resume {
        /// The token.
        token: Token,
    },
    /// Enters without an account, to watch. A guest takes no seat, hosts
    /// nothing and enters no tournament.
    Guest {
        /// A display name, one to twenty-four characters.
        name: String,
    },
}

/// What a participant joins a match as.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// Takes a seat, or comes back to the one this account already holds.
    Player,
    /// Watches without a seat.
    Spectator,
}

/// Where a match is in its life.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// Seats are being taken, heroes picked and readiness declared.
    Gathering,
    /// The simulation runs.
    Playing,
    /// It ended with a winner.
    Over,
    /// Every seat was left and nobody came back in time.
    Abandoned,
}

/// The account a connection signed in as.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AccountInfo {
    /// Which account.
    pub id: AccountId,
    /// Its name.
    pub login: String,
}

/// One of the server's preset rule sets.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ModeInfo {
    /// Which mode.
    pub id: ModeId,
    /// What the hall calls it.
    pub name: String,
    /// Which map it is played on.
    pub map: MapId,
    /// Seats in a match, half per side.
    pub seats: u8,
    /// How the server advances ticks.
    pub tick: TickMode,
    /// Simulation ticks per second.
    pub tick_rate: u16,
    /// Whether cheat orders are honoured.
    pub cheats: bool,
}

/// One seat of a match, as the hall shows it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LobbySlot {
    /// Which seat this row is.
    pub slot: SlotId,
    /// Which side it plays for.
    pub team: Team,
    /// The account holding it. Empty while the seat is open.
    pub login: String,
    /// Which hero has been picked. Absent until one is.
    pub hero: Option<HeroId>,
    /// Whether the holder has declared itself ready.
    pub ready: bool,
    /// Whether the holder is connected right now.
    pub connected: bool,
}

/// A match as its participants see it, sent whenever its composition
/// changes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MatchState {
    /// Which match.
    pub id: MatchId,
    /// What the hall calls it.
    pub name: String,
    /// Which mode it is played under.
    pub mode: ModeId,
    /// The account that may kick from it. Absent for a tournament match.
    pub host: Option<AccountId>,
    /// Where it is in its life.
    pub stage: Stage,
    /// Every seat, sorted by [`SlotId`].
    pub slots: Vec<LobbySlot>,
    /// Display names of everybody in it without a seat.
    pub watchers: Vec<String>,
    /// The seat the receiver holds. Absent for a watcher.
    pub you: Option<SlotId>,
}

/// One seat of a match, as the archive remembers it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SeatSummary {
    /// Which seat.
    pub slot: SlotId,
    /// Which side it played for.
    pub team: Team,
    /// The account that held it. Empty while the seat is open.
    pub login: String,
    /// Which hero was picked. Absent until one is.
    pub hero: Option<HeroId>,
}

/// Which game of which tournament pairing a match is.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TournamentRef {
    /// Which tournament.
    pub id: TournamentId,
    /// Which round, from zero.
    pub round: u8,
    /// Which pairing of the round, from zero.
    pub pair: u8,
    /// Which game of the series, from zero.
    pub game: u8,
}

/// A match as the hall and the archive list it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MatchSummary {
    /// Which match.
    pub id: MatchId,
    /// What the hall calls it.
    pub name: String,
    /// Which mode it is played under.
    pub mode: ModeId,
    /// Which map it is played on.
    pub map: MapId,
    /// Where it is in its life.
    pub stage: Stage,
    /// The account that created it. Absent for a tournament match.
    pub host: Option<AccountId>,
    /// Every seat, sorted by [`SlotId`].
    pub seats: Vec<SeatSummary>,
    /// The tournament game it is. Absent for a match anybody created.
    pub tournament: Option<TournamentRef>,
    /// When it was created, in seconds since the Unix epoch.
    pub created_at: u64,
    /// How many ticks it ran. Zero until it ends.
    pub duration: u32,
    /// Which side won. Absent until it ends with a winner.
    pub winner: Option<Team>,
    /// The seed of its randomness. Absent until it ends.
    pub seed: Option<u64>,
}

/// Which archived matches a listing asks for. Every absent field matches
/// everything.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct MatchFilter {
    /// Only matches in this stage.
    pub stage: Option<Stage>,
    /// Only games of this tournament.
    pub tournament: Option<TournamentId>,
    /// Only matches this account held a seat in.
    pub account: Option<AccountId>,
}

/// Why the server refused something said outside a running match.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HallReason {
    /// The first message was not a [`ClientMsg::Hello`], or a second one came.
    ///
    /// [`ClientMsg::Hello`]: crate::ClientMsg::Hello
    NotSignedIn,
    /// The login or the name breaks the rules for one.
    BadName,
    /// The password is too short or too long.
    BadPassword,
    /// An account with this login exists.
    LoginTaken,
    /// No account has this login.
    NoSuchLogin,
    /// The password does not match.
    WrongPassword,
    /// The token is unknown or has expired.
    UnknownToken,
    /// The account signed in on another connection, and this one is closed.
    SignedInElsewhere,
    /// A guest asked for something only an account may have.
    GuestsCannotPlay,
    /// No mode has this id.
    NoSuchMode,
    /// No match has this id, or it was cancelled.
    NoSuchMatch,
    /// Every seat is held.
    MatchFull,
    /// The match is not gathering any more.
    NotGathering,
    /// Only the host may do this.
    NotHost,
    /// This account holds no seat in the match.
    NoSeat,
    /// The seat asked for is held by somebody else.
    SeatTaken,
    /// The seats of a tournament match are assigned and cannot be changed.
    SeatsFixed,
    /// The connection is in no match.
    NotInMatch,
    /// No tournament has this id.
    NoSuchTournament,
    /// The tournament has started or finished.
    TournamentStarted,
    /// Only the organizer may do this.
    NotOrganizer,
    /// The team has every member it can hold.
    TeamFull,
    /// The tournament has games being played and cannot be taken down.
    TournamentRunning,
    /// The account is in no tournament.
    NotInTournament,
    /// A tournament needs two teams or more to start.
    TooFewTeams,
    /// The account is in a tournament already.
    InTournament,
    /// No replay is stored for this match.
    NoSuchReplay,
    /// The number of games in a series must be odd, one to seven.
    BadSeries,
}

/// How a tournament pairs its teams.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TournamentFormat {
    /// Lose a series and leave; the bracket halves every round.
    SingleElimination,
    /// Every team meets every other once; most series won takes it.
    RoundRobin,
}

/// Where a tournament is in its life.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TournamentStage {
    /// Teams are joining.
    Registration,
    /// Games are being played.
    Running,
    /// A winner is known.
    Finished,
}

/// One team of a tournament.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TeamInfo {
    /// What it is called.
    pub name: String,
    /// Logins of its members.
    pub members: Vec<String>,
}

/// One game of a series.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameInfo {
    /// The match it was played as. Absent for a walkover.
    pub match_id: Option<MatchId>,
    /// Which team won, by its index in the tournament. Absent while it is
    /// being played.
    pub winner: Option<u8>,
}

/// One pairing of a round.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PairInfo {
    /// The first team, by its index. Absent until an earlier round decides
    /// it, or for a bye.
    pub a: Option<u8>,
    /// The second team, by its index.
    pub b: Option<u8>,
    /// The games played so far, in order.
    pub games: Vec<GameInfo>,
    /// Which team took the series. Absent until decided.
    pub winner: Option<u8>,
}

/// One round of a tournament.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RoundInfo {
    /// Its pairings.
    pub pairs: Vec<PairInfo>,
}

/// A tournament whole, sent to its members and to whoever watches it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TournamentInfo {
    /// Which tournament.
    pub id: TournamentId,
    /// What it is called.
    pub name: String,
    /// The mode its games are played under.
    pub mode: ModeId,
    /// How teams are paired.
    pub format: TournamentFormat,
    /// Games in a series: the first team to win more than half takes it.
    pub best_of: u8,
    /// Members a team must have to be paired. A team short of them is left
    /// out when the tournament starts.
    pub team_size: u8,
    /// Login of the account that created it.
    pub organizer: String,
    /// Where it is in its life.
    pub stage: TournamentStage,
    /// Every team, in registration order. Indexes into this name teams
    /// everywhere else.
    pub teams: Vec<TeamInfo>,
    /// Every round, first to last. Empty until the tournament starts.
    pub rounds: Vec<RoundInfo>,
    /// The team that took it, by index. Absent until finished.
    pub winner: Option<u8>,
    /// When it was created, in seconds since the Unix epoch.
    pub created_at: u64,
}

/// A tournament as the hall lists it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TournamentSummary {
    /// Which tournament.
    pub id: TournamentId,
    /// What it is called.
    pub name: String,
    /// The mode its games are played under.
    pub mode: ModeId,
    /// How teams are paired.
    pub format: TournamentFormat,
    /// Games in a series.
    pub best_of: u8,
    /// Where it is in its life.
    pub stage: TournamentStage,
    /// How many teams are in it.
    pub teams: u8,
    /// Members a team must have to be paired.
    pub team_size: u8,
}
