//! The wire between a bot and the servers: signing in and finding a match
//! on the hub, and the messages of one on the game server it plays on.

use std::io::{Read, Write};
use std::marker::PhantomData;
use std::net::TcpStream;

use bota_proto::{
    AccountId, Auth, EntityId, FrameReader, FromGame, FromHub, HallReason, HeroId,
    MAX_ORDERS_PER_MESSAGE, MatchId, Message, ModeId, PickState, PlayerId, Role, Search, SlotId,
    Stage, Ticket, ToGame, ToHub, TournamentId, TournamentStage, UnitOrder, WorldView,
    encode_frame_to_vec,
};

use crate::Ask;

/// Where a bot wants to play.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Want {
    /// Whatever match the queue of a mode makes.
    Quick {
        /// Which mode.
        mode: ModeId,
    },
    /// One match by id, which this account may already hold a seat in.
    Match {
        /// Which match.
        id: MatchId,
    },
    /// Every game a tournament gives this account, as it gives them.
    Tournament {
        /// Which tournament.
        id: TournamentId,
        /// Which team to register in.
        team: String,
    },
}

/// The account a connection signed in as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedIn {
    /// The handle this connection was given.
    pub player: PlayerId,
    /// The account. Absent when the hub took the connection as a guest.
    pub account: Option<AccountId>,
    /// The name it goes by.
    pub login: String,
}

/// The seats a bot was given in one match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seated {
    /// Which match.
    pub match_id: MatchId,
    /// Which seats, in slot order, all on one side. Never empty.
    pub seats: Vec<SlotId>,
}

/// One framed connection to a server that says `M`.
pub struct Wire<M> {
    stream: TcpStream,
    reader: FrameReader,
    hears: PhantomData<fn() -> M>,
}

impl<M: Message> Wire<M> {
    /// Connects without saying anything.
    pub fn connect(addr: &str) -> std::io::Result<Wire<M>> {
        let stream = TcpStream::connect(addr)?;
        stream.set_nodelay(true)?;
        Ok(Wire {
            stream,
            reader: FrameReader::new(),
            hears: PhantomData,
        })
    }

    /// Puts one message on the wire.
    pub fn send<T: Message>(&mut self, msg: &T) -> std::io::Result<()> {
        let frame = encode_frame_to_vec(msg)
            .map_err(|_| std::io::Error::other("a message would not encode"))?;
        self.stream.write_all(&frame)
    }

    /// The next message from the server, waiting for one if it must.
    ///
    /// `None` means the server hung up.
    pub fn hear(&mut self) -> std::io::Result<Option<M>> {
        loop {
            match self.reader.next_message::<M>() {
                Ok(Some(msg)) => return Ok(Some(msg)),
                Ok(None) => {}
                Err(_) => return Err(std::io::Error::other("a message would not decode")),
            }
            let mut buf = [0u8; 65536];
            let read = self.stream.read(&mut buf)?;
            if read == 0 {
                return Ok(None);
            }
            self.reader.push(&buf[..read]);
        }
    }
}

/// A bot's connection to the hub.
pub struct Link {
    wire: Wire<FromHub>,
    /// Matches this connection has gone to play. What the hub still says of
    /// them names no seat to take.
    played: Vec<MatchId>,
}

impl Link {
    /// Connects without saying anything.
    pub fn connect(addr: &str) -> std::io::Result<Link> {
        Ok(Link {
            wire: Wire::connect(addr)?,
            played: Vec::new(),
        })
    }

    /// Signs in, registering the login the first time it is used.
    pub fn sign_in(&mut self, login: &str, password: &str) -> std::io::Result<SignedIn> {
        self.send(&ToHub::Hello {
            auth: Auth::Login {
                login: login.to_string(),
                password: password.to_string(),
            },
        })?;
        loop {
            match self.hear()? {
                None => return Err(std::io::Error::other("the hub hung up on the greeting")),
                Some(FromHub::Welcome {
                    player_id, account, ..
                }) => {
                    return Ok(SignedIn {
                        player: player_id,
                        account: account.as_ref().map(|a| a.id),
                        login: account.map_or_else(|| login.to_string(), |a| a.login),
                    });
                }
                Some(FromHub::Refused {
                    reason: HallReason::NoSuchLogin,
                }) => {
                    self.send(&ToHub::Hello {
                        auth: Auth::Register {
                            login: login.to_string(),
                            password: password.to_string(),
                        },
                    })?;
                }
                Some(FromHub::Refused { reason }) => {
                    return Err(std::io::Error::other(format!(
                        "the hub would not sign this bot in: {reason:?}"
                    )));
                }
                Some(_) => {}
            }
        }
    }

    /// Asks for what the bot wants to play, with a number of seats: in the
    /// queue of a mode, in one match, or in a tournament's team.
    pub fn ask_for(&mut self, want: &Want, seats: u8) -> std::io::Result<()> {
        match want {
            Want::Quick { mode } => self.send(&ToHub::FindMatch { mode: *mode, seats }),
            Want::Match { id } => self.send(&ToHub::JoinMatch {
                id: *id,
                role: Role::Player { seats },
            }),
            Want::Tournament { id, team } => {
                self.send(&ToHub::ViewTournament { id: Some(*id) })?;
                self.send(&ToHub::JoinTournament {
                    id: *id,
                    team: team.clone(),
                    seats,
                })
            }
        }
    }

    /// Waits until the hub seats this connection. A match the queue finds
    /// is accepted.
    ///
    /// `None` means no seat is coming: the tournament ended, the hub
    /// refused, or it hung up.
    pub fn take_seats(&mut self) -> std::io::Result<Option<Seated>> {
        loop {
            match self.hear()? {
                None => return Ok(None),
                Some(FromHub::Search {
                    search:
                        Search::Found {
                            id, yours: false, ..
                        },
                }) => self.send(&ToHub::Accept { id })?,
                Some(FromHub::MatchState { state }) => {
                    let ended = matches!(state.stage, Stage::Over | Stage::Abandoned);
                    if state.yours.is_empty() || ended || self.played.contains(&state.id) {
                        continue;
                    }
                    return Ok(Some(Seated {
                        match_id: state.id,
                        seats: state.yours,
                    }));
                }
                Some(FromHub::TournamentState { state })
                    if state.stage == TournamentStage::Finished =>
                {
                    return Ok(None);
                }
                // An account already in a tournament is one this bot
                // registered before; the games still come.
                Some(FromHub::Refused {
                    reason: HallReason::InTournament,
                }) => {}
                // A tournament taken down brings no more games.
                Some(FromHub::Refused {
                    reason: HallReason::NoSuchTournament,
                }) => return Ok(None),
                Some(FromHub::Refused { reason }) => {
                    return Err(std::io::Error::other(format!(
                        "the hub would not seat this bot: {reason:?}"
                    )));
                }
                Some(_) => {}
            }
        }
    }

    /// Waits for the hub to say where a match plays, and joins it there.
    ///
    /// `None` means the match will not be played on this connection: it
    /// ended or was taken down first, or the hub hung up.
    pub fn wait_to_play(&mut self, match_id: MatchId) -> std::io::Result<Option<Table>> {
        loop {
            match self.hear()? {
                None => return Ok(None),
                Some(FromHub::Play {
                    match_id: id,
                    addr,
                    ticket,
                }) if id == match_id => {
                    self.played.push(match_id);
                    return Table::join(&addr, ticket).map(Some);
                }
                Some(FromHub::MatchState { state })
                    if state.id == match_id
                        && matches!(state.stage, Stage::Over | Stage::Abandoned) =>
                {
                    return Ok(None);
                }
                // Only a connection in no match is shown the hall.
                Some(FromHub::Hall { .. }) => return Ok(None),
                Some(_) => {}
            }
        }
    }

    /// Leaves the match this connection is in.
    pub fn leave_match(&mut self) -> std::io::Result<()> {
        self.send(&ToHub::LeaveMatch)
    }

    /// Puts one message on the wire.
    pub fn send(&mut self, msg: &ToHub) -> std::io::Result<()> {
        self.wire.send(msg)
    }

    /// The next message from the hub, waiting for one if it must.
    ///
    /// `None` means the hub hung up.
    pub fn hear(&mut self) -> std::io::Result<Option<FromHub>> {
        self.wire.hear()
    }
}

/// A bot's connection to the game server of one match.
pub struct Table {
    wire: Wire<FromGame>,
}

impl Table {
    /// Connects to a game server and shows it the ticket the hub gave.
    pub fn join(addr: &str, ticket: Ticket) -> std::io::Result<Table> {
        let mut table = Table {
            wire: Wire::connect(addr)?,
        };
        table.send(&ToGame::Join { ticket })?;
        Ok(table)
    }

    /// Picks a hero for every seat this connection holds that has none and
    /// has not been picked for yet: the nth seat held takes the nth of
    /// `heroes`, starting over when the list runs out. Remembers the seats
    /// picked for in `asked`.
    pub fn pick(
        &mut self,
        state: &PickState,
        heroes: &[HeroId],
        asked: &mut Vec<SlotId>,
    ) -> std::io::Result<()> {
        if heroes.is_empty() {
            return Ok(());
        }
        for (n, slot) in state.yours.iter().enumerate() {
            let open = state
                .seats
                .iter()
                .any(|seat| seat.slot == *slot && seat.hero.is_none());
            if !open || asked.contains(slot) {
                continue;
            }
            self.send(&ToGame::Pick {
                seat: *slot,
                hero: heroes[n % heroes.len()],
            })?;
            asked.push(*slot);
        }
        Ok(())
    }

    /// Sends asks, each for the seat beside it, in as few messages as the
    /// limit allows. Returns the unit each named and its seat.
    ///
    /// An ask for no unit means its seat's hero, found in `view`, standing
    /// or fallen; a seat whose hero has never stood has nobody to name, and
    /// the ask is not sent.
    pub fn orders(
        &mut self,
        asks: &[(SlotId, Ask)],
        view: &WorldView,
    ) -> std::io::Result<Vec<(EntityId, SlotId)>> {
        let mut given = Vec::new();
        let mut sent = Vec::new();
        for (seat, ask) in asks {
            let unit = match ask.unit {
                Some(unit) => unit,
                None => {
                    let hero = view
                        .players
                        .iter()
                        .find(|player| player.slot == *seat)
                        .and_then(|player| player.unit);
                    let Some(hero) = hero else {
                        continue;
                    };
                    hero
                }
            };
            given.push(UnitOrder {
                unit,
                order: ask.order,
            });
            sent.push((unit, *seat));
        }
        for batch in given.chunks(MAX_ORDERS_PER_MESSAGE) {
            self.send(&ToGame::Orders {
                orders: batch.to_vec(),
            })?;
        }
        Ok(sent)
    }

    /// Says every seat held has finished thinking about a tick.
    pub fn done_thinking(&mut self, tick: u32) -> std::io::Result<()> {
        self.send(&ToGame::Ack { tick })
    }

    /// Puts one message on the wire.
    pub fn send(&mut self, msg: &ToGame) -> std::io::Result<()> {
        self.wire.send(msg)
    }

    /// The next message from the game server, waiting for one if it must.
    ///
    /// `None` means the game server hung up.
    pub fn hear(&mut self) -> std::io::Result<Option<FromGame>> {
        self.wire.hear()
    }
}
