//! Seats, who holds them, and the heroes they play.

use bota_proto::{AccountId, HeroId, LobbySlot, PlayerId, SeatSetup, SeatSummary, SlotId, Team};

/// One seat of a match and who holds it.
#[derive(Clone, Debug)]
pub struct RosterSeat {
    /// Which seat.
    pub slot: SlotId,
    /// Which side it plays for.
    pub team: Team,
    /// The account holding it. Absent while open.
    pub account: Option<AccountId>,
    /// Name of the holder. Empty while open.
    pub login: String,
    /// The connection the holder is on. Absent while the holder is away.
    pub player: Option<PlayerId>,
    /// The hero it plays. Absent until its game server says every seat has
    /// picked.
    pub hero: Option<HeroId>,
    /// Whether the holder gave the seat up for good once the match had
    /// started. The account stays named on it; nobody sits on it again.
    pub abandoned: bool,
}

/// All seats of a match.
///
/// An account may hold several, all on one side. The simulation never sees
/// an account or a connection; this is where both stop.
#[derive(Clone, Debug)]
pub struct Roster {
    /// Every seat, sorted by slot.
    pub seats: Vec<RosterSeat>,
}

impl Roster {
    /// An empty roster: even slots are Radiant, odd are Dire.
    pub fn new(players: u8) -> Roster {
        Roster {
            seats: (0..players)
                .map(|i| RosterSeat {
                    slot: SlotId(i),
                    team: if i.is_multiple_of(2) {
                        Team::Radiant
                    } else {
                        Team::Dire
                    },
                    account: None,
                    login: String::new(),
                    player: None,
                    hero: None,
                    abandoned: false,
                })
                .collect(),
        }
    }

    /// The seat at a slot.
    pub fn at(&self, slot: SlotId) -> Option<&RosterSeat> {
        self.seats.iter().find(|s| s.slot == slot)
    }

    /// The seat at a slot, for updating.
    pub fn at_mut(&mut self, slot: SlotId) -> Option<&mut RosterSeat> {
        self.seats.iter_mut().find(|s| s.slot == slot)
    }

    /// The seats an account holds, in slot order. An abandoned seat is not
    /// held.
    pub fn held_by(&self, account: AccountId) -> Vec<SlotId> {
        self.seats
            .iter()
            .filter(|s| s.account == Some(account) && !s.abandoned)
            .map(|s| s.slot)
            .collect()
    }

    /// The seats a connection sits on, in slot order.
    pub fn sat_on_by(&self, player: PlayerId) -> Vec<SlotId> {
        self.seats
            .iter()
            .filter(|s| s.player == Some(player))
            .map(|s| s.slot)
            .collect()
    }

    /// The side an account's seats are on. Absent while it holds none.
    pub fn side_of(&self, account: AccountId) -> Option<Team> {
        self.seats
            .iter()
            .find(|s| s.account == Some(account))
            .map(|s| s.team)
    }

    /// The first open seat, if any.
    pub fn free_seat(&self) -> Option<SlotId> {
        self.seats
            .iter()
            .find(|s| s.account.is_none())
            .map(|s| s.slot)
    }

    /// The side a newcomer asking for a number of seats sits on: of the
    /// sides with room for all of them, the one with the most room, and on
    /// a tie the side of the lower open seat. Absent when neither has room.
    pub fn side_for(&self, seats: u8) -> Option<Team> {
        [Team::Radiant, Team::Dire]
            .into_iter()
            .filter(|team| self.free_on(*team).len() >= usize::from(seats))
            .max_by_key(|team| {
                let free = self.free_on(*team);
                (free.len(), std::cmp::Reverse(free.first().copied()))
            })
    }

    /// Every open seat on a side, in slot order.
    pub fn free_on(&self, team: Team) -> Vec<SlotId> {
        self.seats
            .iter()
            .filter(|s| s.account.is_none() && s.team == team)
            .map(|s| s.slot)
            .collect()
    }

    /// Seats an account on a seat, keeping nothing of who held it.
    pub fn seat(
        &mut self,
        slot: SlotId,
        account: AccountId,
        login: &str,
        player: Option<PlayerId>,
    ) {
        if let Some(seat) = self.at_mut(slot) {
            seat.account = Some(account);
            seat.login = login.to_string();
            seat.player = player;
            seat.hero = None;
            seat.abandoned = false;
        }
    }

    /// Opens one seat.
    pub fn release_slot(&mut self, slot: SlotId) {
        if let Some(seat) = self.at_mut(slot) {
            seat.account = None;
            seat.login.clear();
            seat.player = None;
            seat.hero = None;
            seat.abandoned = false;
        }
    }

    /// Gives up every seat an account holds, for good, and says which.
    pub fn abandon(&mut self, account: AccountId) -> Vec<SlotId> {
        let mut given_up = Vec::new();
        for seat in self
            .seats
            .iter_mut()
            .filter(|s| s.account == Some(account) && !s.abandoned)
        {
            seat.abandoned = true;
            seat.player = None;
            given_up.push(seat.slot);
        }
        given_up
    }

    /// Opens every seat an account holds, and says which.
    pub fn release(&mut self, account: AccountId) -> Vec<SlotId> {
        let held = self.held_by(account);
        for slot in &held {
            self.release_slot(*slot);
        }
        held
    }

    /// Marks every seat a connection sits on away, keeping them its
    /// account's, and says which.
    pub fn away(&mut self, player: PlayerId) -> Vec<SlotId> {
        let mut left = Vec::new();
        for seat in self.seats.iter_mut().filter(|s| s.player == Some(player)) {
            seat.player = None;
            left.push(seat.slot);
        }
        left
    }

    /// Puts a connection back on every seat its account holds, and says
    /// which.
    pub fn sit_back(&mut self, account: AccountId, player: PlayerId) -> Vec<SlotId> {
        let mut back = Vec::new();
        for seat in self
            .seats
            .iter_mut()
            .filter(|s| s.account == Some(account) && !s.abandoned)
        {
            seat.player = Some(player);
            back.push(seat.slot);
        }
        back
    }

    /// Whether every seat is held by an account that is in the match.
    pub fn all_present(&self) -> bool {
        self.seats
            .iter()
            .all(|s| s.account.is_some() && s.player.is_some())
    }

    /// Whether any seat has a holder present.
    pub fn anyone_present(&self) -> bool {
        self.seats.iter().any(|s| s.player.is_some())
    }

    /// Whether any seat is held.
    pub fn anyone_seated(&self) -> bool {
        self.seats.iter().any(|s| s.account.is_some())
    }

    /// The seats as a match state shows them.
    pub fn lobby_slots(&self) -> Vec<LobbySlot> {
        self.seats
            .iter()
            .map(|s| LobbySlot {
                slot: s.slot,
                team: s.team,
                login: s.login.clone(),
                connected: s.player.is_some(),
            })
            .collect()
    }

    /// The seats as the archive remembers them.
    pub fn summaries(&self) -> Vec<SeatSummary> {
        self.seats
            .iter()
            .map(|s| SeatSummary {
                slot: s.slot,
                team: s.team,
                login: s.login.clone(),
                hero: s.hero,
                abandoned: s.abandoned,
            })
            .collect()
    }

    /// The seats as a game server is given them.
    pub fn setups(&self) -> Vec<SeatSetup> {
        self.seats
            .iter()
            .map(|s| SeatSetup {
                slot: s.slot,
                team: s.team,
                login: s.login.clone(),
            })
            .collect()
    }
}
