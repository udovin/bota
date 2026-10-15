//! One connection as the hub knows it.

use bota_proto::{AccountId, MatchId, PlayerId, Token, TournamentId};

/// Who a connection signed in as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Who {
    /// An account.
    Account(AccountId),
    /// A guest with a display name.
    Guest(String),
}

/// Where a connection is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// Signed in, in no match.
    Hall,
    /// In a match, seated or watching.
    Match(MatchId),
}

/// One connection.
#[derive(Clone, Debug)]
pub struct Session {
    /// Which connection.
    pub player: PlayerId,
    /// Who it signed in as. Absent until its Hello is accepted.
    pub who: Option<Who>,
    /// The token it signed in with or was given.
    pub token: Option<Token>,
    /// Sign-in attempts refused on this connection.
    pub failures: u8,
    /// Where it is.
    pub place: Place,
    /// The tournament it asked to be told about.
    pub viewing: Option<TournamentId>,
}

impl Session {
    /// A connection that has said nothing yet.
    pub fn new(player: PlayerId) -> Session {
        Session {
            player,
            who: None,
            token: None,
            failures: 0,
            place: Place::Hall,
            viewing: None,
        }
    }

    /// The account, if signed in as one.
    pub fn account(&self) -> Option<AccountId> {
        match self.who {
            Some(Who::Account(id)) => Some(id),
            _ => None,
        }
    }

    /// The match the connection is in, if any.
    pub fn in_match(&self) -> Option<MatchId> {
        match self.place {
            Place::Match(id) => Some(id),
            Place::Hall => None,
        }
    }
}
