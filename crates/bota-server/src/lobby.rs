//! Seats, picks, readiness, and the mapping between wire and match identity.

use bota_proto::{HeroId, LobbySlot, PlayerId, Role, SlotId, Team};

/// One seat of the match and who currently holds it.
#[derive(Clone, Debug)]
pub struct RosterSeat {
    /// Which seat.
    pub slot: SlotId,
    /// Which side it plays for.
    pub team: Team,
    /// The connection holding it. Absent while open.
    pub player: Option<PlayerId>,
    /// Display name of the holder. Empty while open.
    pub name: String,
    /// What holds it.
    pub role: Option<Role>,
    /// The picked hero.
    pub hero: Option<HeroId>,
    /// Whether the holder declared readiness.
    pub ready: bool,
}

/// All seats, and the `PlayerId` to `SlotId` mapping.
///
/// The simulation never sees a `PlayerId`; this is where network identity
/// stops.
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
                    player: None,
                    name: String::new(),
                    role: None,
                    hero: None,
                    ready: false,
                })
                .collect(),
        }
    }

    /// The seat a connection holds.
    pub fn seat_of(&self, player: PlayerId) -> Option<&RosterSeat> {
        self.seats.iter().find(|s| s.player == Some(player))
    }

    /// The seat a connection holds, for updating.
    pub fn seat_of_mut(&mut self, player: PlayerId) -> Option<&mut RosterSeat> {
        self.seats.iter_mut().find(|s| s.player == Some(player))
    }

    /// The first open seat, if any.
    pub fn free_seat_mut(&mut self) -> Option<&mut RosterSeat> {
        self.seats.iter_mut().find(|s| s.player.is_none())
    }

    /// Releases whatever seat a connection held.
    pub fn release(&mut self, player: PlayerId) -> Option<SlotId> {
        let seat = self.seat_of_mut(player)?;
        seat.player = None;
        seat.name.clear();
        seat.role = None;
        seat.ready = false;
        Some(seat.slot)
    }

    /// Whether every seat is held, picked and ready.
    pub fn all_ready(&self) -> bool {
        self.seats
            .iter()
            .all(|s| s.player.is_some() && s.hero.is_some() && s.ready)
    }

    /// The lobby as the wire sees it.
    pub fn lobby_slots(&self) -> Vec<LobbySlot> {
        self.seats
            .iter()
            .map(|s| LobbySlot {
                slot: s.slot,
                team: s.team,
                name: s.name.clone(),
                role: s.role,
                hero: s.hero,
                ready: s.ready,
            })
            .collect()
    }

    /// The picks, once every seat is ready.
    pub fn picks(&self) -> Vec<bota_proto::Pick> {
        self.seats
            .iter()
            .map(|s| bota_proto::Pick {
                slot: s.slot,
                team: s.team,
                hero: s.hero.expect("picks are read only when all are ready"),
            })
            .collect()
    }
}
