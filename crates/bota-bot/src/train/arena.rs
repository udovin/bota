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

use crate::{Brain, Link, Outcome, Params, play_on};

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
    let yard = open_the_yard(ground, seed)?;
    // Seats go out in the order the server sees the connections arrive, and
    // joining waits for the answer, so the first set here takes the first seat
    // and nothing about the machine can change that.
    let (first, first_seat) = Link::join(&yard.addr, "one", ground.hero)?;
    let (second, second_seat) = Link::join(&yard.addr, "other", ground.hero)?;
    let ticks = ground.ticks;
    let ours = thread::spawn(move || {
        let mut brain = Brain::with(one);
        play_on(&mut brain, first, first_seat, Some(ticks))
    });
    let mut theirs = Brain::with(other);
    let mine = play_on(&mut theirs, second, second_seat, Some(ticks));
    let yours = ours
        .join()
        .map_err(|_| std::io::Error::other("a seat gave up"))?;
    Ok((yours?, mine?))
}
