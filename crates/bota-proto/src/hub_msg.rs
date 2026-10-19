//! What a participant and the hub say to each other: accounts, the hall,
//! matches before and after they run, tournaments and the archive.

use serde::{Deserialize, Serialize};

use crate::{
    AccountId, AccountInfo, Auth, HallReason, MatchFilter, MatchId, MatchState, MatchSummary,
    ModeId, ModeInfo, PlayerId, Role, Search, SlotId, Team, Ticket, Token, TournamentFormat,
    TournamentId, TournamentInfo, TournamentSummary,
};

/// Anything a participant can say to the hub.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ToHub {
    /// First message on a connection, before anything else is accepted.
    Hello {
        /// Who this connection is.
        auth: Auth,
    },
    /// Replace the account's password.
    ChangePassword {
        /// The password as it is.
        old: String,
        /// The password as it will be.
        new: String,
    },
    /// Forget the token this connection signed in with, or was given.
    Logout,
    /// Ask for a page of the archive, newest first.
    ListMatches {
        /// Which matches.
        filter: MatchFilter,
        /// Only matches with a smaller id. Absent starts from the newest.
        before: Option<MatchId>,
        /// At most this many.
        limit: u16,
    },
    /// Open a match and take its first seats, on the Radiant. Leaves
    /// whatever match the connection was in.
    CreateMatch {
        /// Which mode.
        mode: ModeId,
        /// What the hall will call it.
        name: String,
        /// The seed of its randomness. Absent draws a fresh one.
        seed: Option<u64>,
        /// How many seats to take, one or more, up to the mode's
        /// [`ModeInfo::seats_per_player`](crate::ModeInfo::seats_per_player).
        seats: u8,
    },
    /// Wait in the matchmaking queue of a mode. Once the queue holds enough
    /// connections to fill a match, each is asked to accept it. Leaves
    /// whatever gathering match the connection was in.
    FindMatch {
        /// Which mode.
        mode: ModeId,
        /// How many seats to take, all on one side, one or more, up to the
        /// mode's [`ModeInfo::seats_per_player`](crate::ModeInfo::seats_per_player).
        seats: u8,
    },
    /// Leave the matchmaking queue, or turn down a match found in it.
    CancelSearch,
    /// Accept a match found in the queue.
    Accept {
        /// Which match.
        id: MatchId,
    },
    /// Turn down a match found in the queue, and leave the queue.
    Decline {
        /// Which match.
        id: MatchId,
    },
    /// Enter a match. Leaves whatever match the connection was in. A match
    /// on its game server, picking or playing, is answered with a
    /// [`FromHub::Play`].
    JoinMatch {
        /// Which match.
        id: MatchId,
        /// With a seat or without.
        role: Role,
    },
    /// Leave the match the connection is in, back to the hall. A seat in a
    /// match that has started stays this account's, and so does a seat a
    /// tournament or the queue gave.
    LeaveMatch,
    /// Send the gathering match the connection is in to a game server,
    /// where its heroes are picked and it is played. Host only, once every
    /// seat is held by an account that is in the match.
    StartMatch,
    /// Give up every seat this account holds in a match that has started,
    /// for good. The heroes stay where they are; a side whose every seat is
    /// abandoned loses.
    Abandon {
        /// Which match.
        id: MatchId,
    },
    /// Take one more seat of a gathering match, on the side of the seats
    /// already held, within the mode's
    /// [`ModeInfo::seats_per_player`](crate::ModeInfo::seats_per_player).
    TakeSeat {
        /// Which seat.
        slot: SlotId,
    },
    /// Give up a seat of a gathering match. A connection left with none
    /// watches.
    DropSeat {
        /// Which seat. Absent gives up all of them.
        slot: Option<SlotId>,
    },
    /// Throw an account out of a gathering match. Host only.
    Kick {
        /// Which account.
        account: AccountId,
    },
    /// Open a tournament for registration.
    CreateTournament {
        /// What it is called.
        name: String,
        /// The mode its games are played under.
        mode: ModeId,
        /// How teams are paired.
        format: TournamentFormat,
        /// Games in a series: one, three, five or seven.
        best_of: u8,
    },
    /// Register in a tournament, in a team by name. A new name makes the
    /// team, a known name joins it.
    JoinTournament {
        /// Which tournament.
        id: TournamentId,
        /// Which team.
        team: String,
        /// How many of the team's seats to hold in every game, one or more,
        /// up to the mode's
        /// [`ModeInfo::seats_per_player`](crate::ModeInfo::seats_per_player).
        seats: u8,
    },
    /// Withdraw from the tournament this account is registered in, while it
    /// still registers.
    LeaveTournament,
    /// Close registration and pair the teams. Organizer only.
    StartTournament {
        /// Which tournament.
        id: TournamentId,
    },
    /// Take a tournament down before it starts. Organizer only.
    CancelTournament {
        /// Which tournament.
        id: TournamentId,
    },
    /// Receive the state of a tournament whenever it changes. Absent stops.
    ViewTournament {
        /// Which tournament.
        id: Option<TournamentId>,
    },
    /// Ask for the replay of a match, as a run of [`FromHub::ReplayChunk`]
    /// ending in a [`FromHub::ReplayEnd`].
    FetchReplay {
        /// Which match.
        match_id: MatchId,
    },
    /// Say a line in the hall, or in the match the connection is in while it
    /// is not on its game server. A line starting with `/` is a command the
    /// hub runs instead.
    Chat {
        /// What is said, at most [`MAX_CHAT_CHARS`](crate::MAX_CHAT_CHARS)
        /// characters.
        text: String,
        /// Said to the speaker's own side alone. Not read for a command, nor
        /// in the hall.
        team: bool,
    },
}

/// Anything the hub can say to a participant.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum FromHub {
    /// Accepts a [`ToHub::Hello`].
    Welcome {
        /// Handle for this connection.
        player_id: PlayerId,
        /// The account signed in. Absent for a guest.
        account: Option<AccountInfo>,
        /// A key that signs the same account in again. Absent for a guest.
        token: Option<Token>,
        /// Every mode the server offers, in id order.
        modes: Vec<ModeInfo>,
    },
    /// The match the connection is in, resent whenever its composition or
    /// stage changes. Receiving it means being in that match.
    MatchState {
        /// The match.
        state: MatchState,
    },
    /// The match the connection is in is played on a game server: connect
    /// there and say [`ToGame::Join`](crate::ToGame::Join) with the ticket.
    /// Sent when it starts, and again to whoever enters it or comes back
    /// while it runs.
    Play {
        /// Which match.
        match_id: MatchId,
        /// Where its game server takes players, as `host:port`.
        addr: String,
        /// What lets this connection in.
        ticket: Ticket,
    },
    /// Something said to the hub was not accepted.
    Refused {
        /// Why.
        reason: HallReason,
    },
    /// What the hall holds, sent on entering it and whenever it changes.
    /// Receiving it means being in the hall.
    Hall {
        /// Every match gathering, playing or lately ended, newest first.
        matches: Vec<MatchSummary>,
        /// Every tournament not yet finished, and the lately finished ones.
        tournaments: Vec<TournamentSummary>,
    },
    /// A page of the archive, answering a [`ToHub::ListMatches`].
    Matches {
        /// The matches, newest first.
        page: Vec<MatchSummary>,
        /// Where the next page starts. Absent when this was the last.
        before: Option<MatchId>,
    },
    /// A tournament whole, sent to its members and to whoever views it,
    /// whenever it changes.
    TournamentState {
        /// The tournament.
        state: TournamentInfo,
    },
    /// A piece of a replay file, in file order.
    ReplayChunk {
        /// Which match.
        match_id: MatchId,
        /// Where in the file this piece starts.
        offset: u64,
        /// The bytes.
        bytes: Vec<u8>,
    },
    /// The replay file has been sent whole.
    ReplayEnd {
        /// Which match.
        match_id: MatchId,
        /// The size of the file.
        total: u64,
    },
    /// Somebody said a line where the receiver is: in the hall, or in its
    /// match before it starts or after it ends. The speaker hears its own
    /// line too.
    Chat {
        /// Who said it: an account's login.
        from: String,
        /// The side the speaker plays for. Absent for a spectator and in the
        /// hall.
        side: Option<Team>,
        /// Said to that side alone.
        team: bool,
        /// What was said.
        text: String,
    },
    /// A line from the hub itself: the answer to a command, or word to
    /// everybody present that one was run.
    Notice {
        /// What it says.
        text: String,
    },
    /// Where the connection stands in matchmaking, sent whenever that
    /// changes.
    Search {
        /// Where.
        search: Search,
    },
}
