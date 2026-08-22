//! A match played in this process, with no server and no socket.
//!
//! The one place in the bot that reaches into `bota-server`, and the only one
//! that ever should. It builds a world, walks it, and turns each tick into
//! exactly the messages a server would have sent, so nothing above it can tell
//! the difference and nothing above it gains anything from the simulation
//! being in the same process.
//!
//! Why it exists: measured over one match of twenty thousand ticks, the world
//! steps in 231 ms and the fog costs 46 ms, while postcard, TCP, a second
//! process and the acks between them come to 5812 ms. The game is three per
//! cent of a match; this is most of the rest taken out.
//!
//! What it must never become is a shortcut into the simulation. A bench hands
//! over the view of that seat's own side, fog and all, which is byte for byte
//! what the socket would have carried. Handing over the world itself would be
//! faster still and would teach a bot that cannot play, having learned on what
//! a seat is never shown.
//!
//! Two things here were got wrong first and found by playing one seed both
//! ways. A tick has to wait for every seat still at the table, including one
//! that has not spoken yet. And there is no snapshot of the tick a match
//! begins on: a server gathers orders, advances, and only then sends, so the
//! first snapshot a seat ever sees is of tick one.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

use bota_proto::{EventKind, HeroId, MapId, Pick, ServerMsg, SlotId, Team, TickMode};
use bota_server::game::{Command, EventVisibility, MatchConfig, World};

use crate::{Ask, Seated, Wire};

/// Ticks a second. Nothing here waits on a clock, but a match reckons its own
/// time by it.
const TICK_RATE: u16 = 30;

/// One seat's end of a match played here.
///
/// Answers the same three questions a socket does, so the seat loop drives one
/// without knowing the difference.
pub struct Bench {
    shared: Arc<Table>,
    slot: SlotId,
    waiting: VecDeque<ServerMsg>,
}

/// Sets a match up and hands back a bench a seat, in slot order.
pub fn sit_down(seed: u64, map: u16, hero: HeroId, seats: usize) -> (Vec<Bench>, Vec<Seated>) {
    let seats = seats.max(1);
    let shared = Arc::new(Table::new(seed, map, hero, seats));
    let mut benches = Vec::with_capacity(seats);
    let mut seated = Vec::with_capacity(seats);
    for at in 0..seats {
        let slot = SlotId(at as u8);
        benches.push(Bench {
            shared: Arc::clone(&shared),
            slot,
            waiting: VecDeque::new(),
        });
        seated.push(Seated {
            player: bota_proto::PlayerId(at as u32),
            slot: Some(slot),
            tick_rate: TICK_RATE,
            // Lockstep with the sockets taken out: a tick moves when every
            // bench has said it has finished thinking.
            mode: TickMode::Lockstep,
        });
    }
    (benches, seated)
}

/// The match everybody is sitting at.
struct Table {
    state: Mutex<Play>,
    moved: Condvar,
    seats: usize,
}

struct Play {
    world: World,
    picks: Vec<Pick>,
    /// What each seat has still to be told.
    post: Vec<VecDeque<ServerMsg>>,
    /// What each seat has asked for this tick, if anything.
    asked: Vec<Option<Command>>,
    /// Which tick each seat has finished thinking about. Nothing yet for a
    /// seat that has not spoken, which is not the same as one that has thought
    /// about everything: a tick waits for both.
    thought: Vec<Option<u32>>,
    /// Which seats have got up. A tick does not wait on an empty bench.
    gone: Vec<bool>,
    /// How many orders each seat has sent, which is what names one.
    sent: Vec<u32>,
    /// Whether the match has ended.
    done: bool,
}

impl Table {
    /// Sets a match up and seats everybody.
    fn new(seed: u64, map: u16, hero: HeroId, seats: usize) -> Table {
        let cfg = config_for(seed, map, hero, seats);
        let mut world = World::for_match(&cfg, cfg.rng());
        let start = ServerMsg::MatchStart { info: cfg.info() };
        // One tick before anybody is shown anything, because that is what a
        // server does: gather orders, advance, and only then send.
        world.advance(&[]);
        let picks = cfg.picks.clone();
        let mut post: Vec<VecDeque<ServerMsg>> = Vec::with_capacity(seats);
        for at in 0..seats {
            let mut mine = VecDeque::new();
            mine.push_back(start.clone());
            mine.push_back(ServerMsg::Snapshot {
                view: world.view(side_of(&picks, SlotId(at as u8))),
            });
            post.push(mine);
        }
        Table {
            seats,
            moved: Condvar::new(),
            state: Mutex::new(Play {
                world,
                picks,
                post,
                asked: vec![None; seats],
                thought: vec![None; seats],
                gone: vec![false; seats],
                sent: vec![0; seats],
                done: false,
            }),
        }
    }
}

impl Wire for Bench {
    fn hear(&mut self) -> std::io::Result<Option<ServerMsg>> {
        loop {
            if let Some(msg) = self.waiting.pop_front() {
                return Ok(Some(msg));
            }
            let mut play = self.shared.state.lock().expect("a table lock");
            let mine = usize::from(self.slot.0);
            if !play.post[mine].is_empty() {
                self.waiting = std::mem::take(&mut play.post[mine]);
                continue;
            }
            if play.done {
                return Ok(None);
            }
            // Nothing more to say until the tick moves, and the tick moves
            // when everybody still here has finished thinking about this one.
            if play.everybody_thought() {
                play.step(self.shared.seats);
                self.shared.moved.notify_all();
                continue;
            }
            let _unused = self.shared.moved.wait(play).expect("a table lock");
        }
    }

    fn order(&mut self, ask: Ask) -> std::io::Result<()> {
        let mut play = self.shared.state.lock().expect("a table lock");
        let mine = usize::from(self.slot.0);
        play.sent[mine] += 1;
        let seq = play.sent[mine];
        // A refusal comes back on this seat's own stream, which is how the
        // wire answers one, so a seat counts its refusals the same either way.
        match play.world.validate_order(self.slot, ask.unit, &ask.order) {
            Err(reason) => play.post[mine].push_back(ServerMsg::OrderRejected { seq, reason }),
            Ok(()) => {
                play.asked[mine] = Some(Command {
                    slot: self.slot,
                    unit: ask.unit,
                    order: ask.order,
                });
            }
        }
        Ok(())
    }

    fn done_thinking(&mut self, tick: u32) -> std::io::Result<()> {
        let mut play = self.shared.state.lock().expect("a table lock");
        play.thought[usize::from(self.slot.0)] = Some(tick);
        if play.everybody_thought() {
            play.step(self.shared.seats);
        }
        self.shared.moved.notify_all();
        Ok(())
    }
}

impl Drop for Bench {
    /// Gets up, so that whoever is left does not wait on an empty bench.
    ///
    /// Over the wire this is what the straggler timeout is for. Here a seat
    /// that has finished simply stops being one a tick waits on, which is the
    /// same thing with no clock in it.
    fn drop(&mut self) {
        if let Ok(mut play) = self.shared.state.lock() {
            play.gone[usize::from(self.slot.0)] = true;
        }
        self.shared.moved.notify_all();
    }
}

impl Play {
    /// Whether every seat still at the table has finished thinking about the
    /// tick just sent.
    fn everybody_thought(&self) -> bool {
        let now = self.world.tick;
        self.thought
            .iter()
            .zip(&self.gone)
            .all(|(at, gone)| *gone || at.is_some_and(|at| at >= now))
    }

    /// Moves the match on one tick and tells everybody what happened.
    ///
    /// The same order a server puts on the wire: the snapshot of the new tick,
    /// then the events of it this seat is allowed to know about, then the end
    /// if it has come.
    fn step(&mut self, seats: usize) {
        if self.done {
            return;
        }
        let cmds: Vec<Command> = self.asked.iter().flatten().copied().collect();
        self.asked.fill(None);
        let events = self.world.advance(&cmds);
        for at in 0..seats {
            let team = side_of(&self.picks, SlotId(at as u8));
            self.post[at].push_back(ServerMsg::Snapshot {
                view: self.world.view(team),
            });
            let visible: Vec<EventKind> = events
                .iter()
                .filter(|happened| match happened.visible_to {
                    EventVisibility::Everyone => true,
                    EventVisibility::OneTeam(only) => only == team,
                })
                .map(|happened| happened.kind.clone())
                .collect();
            if !visible.is_empty() {
                self.post[at].push_back(ServerMsg::Events {
                    tick: self.world.tick,
                    events: visible,
                });
            }
        }
        if let Some(winner) = self.world.victor() {
            let over = ServerMsg::MatchOver {
                winner,
                stats: self.world.match_stats(),
            };
            for at in 0..seats {
                self.post[at].push_back(over.clone());
            }
            self.done = true;
        }
    }
}

/// Which side a seat plays for.
fn side_of(picks: &[Pick], slot: SlotId) -> Team {
    picks
        .iter()
        .find(|pick| pick.slot == slot)
        .map_or(Team::Radiant, |pick| pick.team)
}

/// The match settings, put together the way a lobby puts them together.
///
/// Even slots play for the Radiant and odd for the Dire, and the key the whole
/// match is drawn from is the seed laid end to end. Both are what the server
/// does with the same numbers, and a match that differed in either would not
/// be the match the bot is going to play.
fn config_for(seed: u64, map: u16, hero: HeroId, seats: usize) -> MatchConfig {
    let mut master_key = [0u8; 32];
    for (at, byte) in seed.to_le_bytes().iter().cycle().take(32).enumerate() {
        master_key[at] = *byte;
    }
    let picks = (0..seats)
        .map(|at| Pick {
            slot: SlotId(at as u8),
            team: if at % 2 == 0 {
                Team::Radiant
            } else {
                Team::Dire
            },
            hero,
        })
        .collect();
    MatchConfig {
        match_id: seed,
        master_key,
        picks,
        map: MapId(map),
        tick_rate: TICK_RATE,
        mode: TickMode::Lockstep,
        ack_timeout_ticks: 0,
    }
}
