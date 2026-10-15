//! Seats, who holds them, picks and readiness.

use bota_proto::{AccountId, HeroId, LobbySlot, Pick, PlayerId, SeatSummary, SlotId, Team};

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
    /// The picked hero.
    pub hero: Option<HeroId>,
    /// Whether the holder declared readiness.
    pub ready: bool,
}

/// All seats of a match.
///
/// The simulation never sees an account or a connection; this is where both
/// stop.
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
                    ready: false,
                })
                .collect(),
        }
    }

    /// The seat an account holds.
    pub fn seat_of(&self, account: AccountId) -> Option<&RosterSeat> {
        self.seats.iter().find(|s| s.account == Some(account))
    }

    /// The seat an account holds, for updating.
    pub fn seat_of_mut(&mut self, account: AccountId) -> Option<&mut RosterSeat> {
        self.seats.iter_mut().find(|s| s.account == Some(account))
    }

    /// The seat a connection sits on.
    pub fn seat_of_player(&self, player: PlayerId) -> Option<&RosterSeat> {
        self.seats.iter().find(|s| s.player == Some(player))
    }

    /// The first open seat, if any.
    pub fn free_seat_mut(&mut self) -> Option<&mut RosterSeat> {
        self.seats.iter_mut().find(|s| s.account.is_none())
    }

    /// Seats an account on a seat, keeping nothing of who held it.
    pub fn seat(
        &mut self,
        slot: SlotId,
        account: AccountId,
        login: &str,
        player: Option<PlayerId>,
    ) {
        if let Some(seat) = self.seats.iter_mut().find(|s| s.slot == slot) {
            seat.account = Some(account);
            seat.login = login.to_string();
            seat.player = player;
            seat.hero = None;
            seat.ready = false;
        }
    }

    /// Opens whatever seat an account held.
    pub fn release(&mut self, account: AccountId) -> Option<SlotId> {
        let seat = self.seat_of_mut(account)?;
        seat.account = None;
        seat.login.clear();
        seat.player = None;
        seat.hero = None;
        seat.ready = false;
        Some(seat.slot)
    }

    /// Marks the holder of a seat away, keeping the seat theirs.
    pub fn away(&mut self, player: PlayerId) -> Option<SlotId> {
        let seat = self.seats.iter_mut().find(|s| s.player == Some(player))?;
        seat.player = None;
        seat.ready = false;
        Some(seat.slot)
    }

    /// Whether every seat is held, present, picked and ready.
    pub fn all_ready(&self) -> bool {
        self.seats
            .iter()
            .all(|s| s.account.is_some() && s.player.is_some() && s.hero.is_some() && s.ready)
    }

    /// Whether every seat is held, present and picked, ready or not.
    pub fn all_present(&self) -> bool {
        self.seats
            .iter()
            .all(|s| s.account.is_some() && s.player.is_some() && s.hero.is_some())
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
                hero: s.hero,
                ready: s.ready,
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
            })
            .collect()
    }

    /// The picks, once every seat has one.
    pub fn picks(&self) -> Vec<Pick> {
        self.seats
            .iter()
            .map(|s| Pick {
                slot: s.slot,
                team: s.team,
                hero: s.hero.expect("picks are read only when all are picked"),
            })
            .collect()
    }
}
