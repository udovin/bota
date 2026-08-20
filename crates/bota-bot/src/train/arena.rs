//! One match, start to finish, without a person in it.
//!
//! A server plays one match and exits, so a bout owns a server of its own: it
//! is started on a port the system picks, both seats are taken by brains in
//! threads of their own, and it is shut down with the bout whatever happened.
//!
//! Lockstep is what makes this worth doing. The server advances as soon as
//! every seat has acknowledged the tick, so a match runs as fast as the two
//! brains can think rather than at the pace a person would watch it.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;

use bota_proto::HeroId;

use crate::{Bot, Brain, Link, Outcome, Params, play_on};

/// How to start a server for one bout.
#[derive(Clone, Debug)]
pub struct Ground {
    /// The server to run.
    pub server: PathBuf,
    /// Which map to play.
    pub map: u16,
    /// Ticks a second, which in lockstep only sets how long a straggler is
    /// waited for.
    pub tick_rate: u16,
    /// Tick-lengths to wait for a straggler before moving on.
    pub ack_timeout_ticks: u32,
    /// Which hero both seats ask for.
    pub hero: HeroId,
    /// Ticks to play before both seats leave.
    pub ticks: u32,
}

impl Default for Ground {
    fn default() -> Ground {
        Ground {
            server: server_beside_us(),
            map: 0,
            tick_rate: 30,
            ack_timeout_ticks: 20,
            hero: HeroId(0),
            ticks: 9000,
        }
    }
}

/// The server binary built alongside this one.
pub fn server_beside_us() -> PathBuf {
    let named = if cfg!(windows) {
        "bota-server.exe"
    } else {
        "bota-server"
    };
    std::env::current_exe()
        .ok()
        .and_then(|me| me.parent().map(|dir| dir.join(named)))
        .unwrap_or_else(|| PathBuf::from(named))
}

/// A server running one match, killed when this is dropped.
struct Yard {
    /// The process itself.
    child: Child,
    /// Where it listens.
    addr: String,
}

impl Drop for Yard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts a server and waits until it says where it listens.
fn open_the_yard(ground: &Ground, seed: u64) -> std::io::Result<Yard> {
    if !Path::new(&ground.server).exists() {
        return Err(std::io::Error::other(format!(
            "no server at {}",
            ground.server.display()
        )));
    }
    let mut child = Command::new(&ground.server)
        .arg("--port")
        .arg("0")
        .arg("--mode")
        .arg("lockstep")
        .arg("--players")
        .arg("2")
        .arg("--tick-rate")
        .arg(ground.tick_rate.to_string())
        .arg("--map")
        .arg(ground.map.to_string())
        .arg("--seed")
        .arg(seed.to_string())
        .arg("--ack-timeout-ticks")
        .arg(ground.ack_timeout_ticks.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let Some(out) = child.stdout.take() else {
        return Err(std::io::Error::other("the server said nothing"));
    };
    let mut line = String::new();
    BufReader::new(out).read_line(&mut line)?;
    let Some(port) = line.rsplit(':').next().map(str::trim) else {
        return Err(std::io::Error::other("the server named no port"));
    };
    if port.parse::<u16>().is_err() {
        return Err(std::io::Error::other(format!(
            "the server said {line:?}, which names no port"
        )));
    }
    Ok(Yard {
        child,
        addr: format!("127.0.0.1:{port}"),
    })
}

/// Plays one match between two sets of numbers.
///
/// The first set takes the first seat the server hands out, which is the first
/// side; a fair comparison plays the pair twice with the sets swapped.
pub fn bout(
    ground: &Ground,
    seed: u64,
    one: Params,
    other: Params,
) -> std::io::Result<(Outcome, Outcome)> {
    let mut first = Brain::with(one);
    let mut second = Brain::with(other);
    bout_between(ground, seed, &mut first, &mut second)
}

/// The same, between any two bots at all.
///
/// The bots are borrowed rather than taken, so whatever they gathered during
/// the match is still there to read afterwards. Both seats are joined before
/// either plays: seats go out in the order the server sees connections arrive,
/// and which side a bot plays has to be the caller's decision rather than the
/// scheduler's.
pub fn bout_between<A, B>(
    ground: &Ground,
    seed: u64,
    one: &mut A,
    other: &mut B,
) -> std::io::Result<(Outcome, Outcome)>
where
    A: Bot + Send,
    B: Bot + Send,
{
    let yard = open_the_yard(ground, seed)?;
    let (first, first_seat) = Link::join(&yard.addr, "one", ground.hero)?;
    let (second, second_seat) = Link::join(&yard.addr, "other", ground.hero)?;
    let ticks = ground.ticks;
    let played = thread::scope(|scope| {
        let ours = scope.spawn(|| play_on(one, first, first_seat, Some(ticks)));
        let mine = play_on(other, second, second_seat, Some(ticks));
        let yours = ours
            .join()
            .unwrap_or_else(|_| Err(std::io::Error::other("a seat gave up")));
        (yours, mine)
    });
    Ok((played.0?, played.1?))
}
