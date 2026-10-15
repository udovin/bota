//! The wire between a bot and a server: signing in, finding a match, and
//! the messages of one.

use std::io::{Read, Write};
use std::net::TcpStream;

use bota_proto::{
    AccountId, Auth, ClientMsg, FrameReader, HallReason, HeroId, MatchId, ModeId, PlayerId, Role,
    ServerMsg, SlotId, Stage, TournamentId, TournamentStage, encode_frame_to_vec,
};

use crate::Ask;

/// Where a bot wants to play.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Want {
    /// Any open match of a mode, opening one when none has a seat free.
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
    /// The account. Absent when the server took the connection as a guest.
    pub account: Option<AccountId>,
    /// The name it goes by.
    pub login: String,
}

/// The seat a bot was given in one match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seated {
    /// Which match.
    pub match_id: MatchId,
    /// Which seat.
    pub slot: SlotId,
}

/// One framed connection to a server.
pub struct Link {
    stream: TcpStream,
    reader: FrameReader,
    seq: u32,
}

impl Link {
    /// Connects without saying anything.
    pub fn connect(addr: &str) -> std::io::Result<Link> {
        let stream = TcpStream::connect(addr)?;
        stream.set_nodelay(true)?;
        Ok(Link {
            stream,
            reader: FrameReader::new(),
            seq: 0,
        })
    }

    /// Signs in, registering the login the first time it is used.
    pub fn sign_in(&mut self, login: &str, password: &str) -> std::io::Result<SignedIn> {
        self.send(&ClientMsg::Hello {
            auth: Auth::Login {
                login: login.to_string(),
                password: password.to_string(),
            },
        })?;
        loop {
            match self.hear()? {
                None => return Err(std::io::Error::other("the server hung up on the greeting")),
                Some(ServerMsg::Welcome {
                    player_id, account, ..
                }) => {
                    return Ok(SignedIn {
                        player: player_id,
                        account: account.as_ref().map(|a| a.id),
                        login: account.map_or_else(|| login.to_string(), |a| a.login),
                    });
                }
                Some(ServerMsg::Refused {
                    reason: HallReason::NoSuchLogin,
                }) => {
                    self.send(&ClientMsg::Hello {
                        auth: Auth::Register {
                            login: login.to_string(),
                            password: password.to_string(),
                        },
                    })?;
                }
                Some(ServerMsg::Refused { reason }) => {
                    return Err(std::io::Error::other(format!(
                        "the server would not sign this bot in: {reason:?}"
                    )));
                }
                Some(_) => {}
            }
        }
    }

    /// Asks for what the bot wants to play.
    pub fn ask_for(&mut self, want: &Want) -> std::io::Result<()> {
        match want {
            Want::Quick { mode } => self.send(&ClientMsg::QuickPlay { mode: *mode }),
            Want::Match { id } => self.send(&ClientMsg::JoinMatch {
                id: *id,
                role: Role::Player,
            }),
            Want::Tournament { id, team } => {
                self.send(&ClientMsg::ViewTournament { id: Some(*id) })?;
                self.send(&ClientMsg::JoinTournament {
                    id: *id,
                    team: team.clone(),
                })
            }
        }
    }

    /// Waits until the server seats this connection, picks a hero and
    /// declares readiness.
    ///
    /// `None` means no seat is coming: the tournament ended, the server
    /// refused, or it hung up.
    pub fn take_seat(&mut self, hero: HeroId) -> std::io::Result<Option<Seated>> {
        loop {
            match self.hear()? {
                None => return Ok(None),
                Some(ServerMsg::MatchState { state }) => {
                    let Some(slot) = state.you else {
                        continue;
                    };
                    if state.stage == Stage::Gathering {
                        self.send(&ClientMsg::PickHero { hero })?;
                        self.send(&ClientMsg::SetReady(true))?;
                    }
                    return Ok(Some(Seated {
                        match_id: state.id,
                        slot,
                    }));
                }
                Some(ServerMsg::TournamentState { state })
                    if state.stage == TournamentStage::Finished =>
                {
                    return Ok(None);
                }
                // An account already in a tournament is one this bot
                // registered before; the games still come.
                Some(ServerMsg::Refused {
                    reason: HallReason::InTournament,
                }) => {}
                // A tournament taken down brings no more games.
                Some(ServerMsg::Refused {
                    reason: HallReason::NoSuchTournament,
                }) => return Ok(None),
                Some(ServerMsg::Refused { reason }) => {
                    return Err(std::io::Error::other(format!(
                        "the server would not seat this bot: {reason:?}"
                    )));
                }
                Some(_) => {}
            }
        }
    }

    /// Leaves the match this connection is in.
    pub fn leave_match(&mut self) -> std::io::Result<()> {
        self.send(&ClientMsg::LeaveMatch)
    }

    /// Puts one message on the wire.
    pub fn send(&mut self, msg: &ClientMsg) -> std::io::Result<()> {
        let frame = encode_frame_to_vec(msg)
            .map_err(|_| std::io::Error::other("a message would not encode"))?;
        self.stream.write_all(&frame)
    }

    /// Sends one ask under the next sequence number.
    pub fn order(&mut self, ask: Ask) -> std::io::Result<()> {
        self.seq += 1;
        let seq = self.seq;
        self.send(&ClientMsg::Order {
            seq,
            unit: ask.unit,
            order: ask.order,
        })
    }

    /// Says this seat has finished thinking about a tick.
    pub fn done_thinking(&mut self, tick: u32) -> std::io::Result<()> {
        self.send(&ClientMsg::Ack { tick })
    }

    /// The next message from the server, waiting for one if it must.
    ///
    /// `None` means the server hung up.
    pub fn hear(&mut self) -> std::io::Result<Option<ServerMsg>> {
        loop {
            match self.reader.next_message::<ServerMsg>() {
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
