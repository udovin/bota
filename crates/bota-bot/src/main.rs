//! Command line entry point of the bot.

use std::rc::Rc;

use bota_bot::{Chair, Crew, Outcome, Playbook, Role, Squad, Want, play_crew, play_every};
use bota_proto::{HeroId, MatchId, ModeId, TournamentId};
use clap::Parser;

/// bota bot: signs in, takes seats as any player does, and plays.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Where the hub listens.
    #[arg(long, default_value = "127.0.0.1:4455")]
    addr: String,
    /// The account to play as. Registered the first time it is used.
    #[arg(long, default_value = "bot")]
    login: String,
    /// Its password.
    #[arg(long, default_value = "bot")]
    password: String,
    /// Wait in the queue of this mode and play the match it makes.
    #[arg(long, default_value_t = 0, conflicts_with_all = ["match_id", "tournament"])]
    mode: u16,
    /// Take seats in this match instead.
    #[arg(long = "match", value_name = "ID")]
    match_id: Option<u64>,
    /// Play every game this tournament gives.
    #[arg(long, value_name = "ID", requires = "team")]
    tournament: Option<u64>,
    /// Which team to register in the tournament as.
    #[arg(long)]
    team: Option<String>,
    /// How many seats to take, all on one side and played on one
    /// connection, within what the mode lets one account hold.
    #[arg(long, default_value_t = 1)]
    seats: u8,
    /// Heroes to pick, seat by seat, comma separated: 0 Sylla, 1 Pudge,
    /// 2 Shadow Fiend. A list shorter than the seats starts over.
    #[arg(long, value_delimiter = ',', default_value = "2")]
    hero: Vec<u16>,
    /// What each seat is there to do, one to five, comma separated, seat by
    /// seat. A list shorter than the seats starts over.
    #[arg(long, value_delimiter = ',', default_value = "2")]
    role: Vec<u8>,
    /// Leave after this many ticks.
    #[arg(long, value_name = "TICKS")]
    limit: Option<u32>,
    /// Write a line about what each seat is doing every so many ticks.
    #[arg(long, value_name = "TICKS", default_value_t = 0)]
    trace: u32,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let mut roles = Vec::new();
    for number in &args.role {
        let Some(role) = Role::of(*number) else {
            return Err(std::io::Error::other(format!(
                "there is no role {number}: they are numbered one to five"
            )));
        };
        roles.push(role);
    }
    let heroes: Vec<HeroId> = args.hero.iter().map(|id| HeroId(*id)).collect();
    let want = match (args.match_id, args.tournament) {
        (Some(id), _) => Want::Match { id: MatchId(id) },
        (None, Some(id)) => Want::Tournament {
            id: TournamentId(id),
            team: args.team.clone().unwrap_or_else(|| args.login.clone()),
        },
        (None, None) => Want::Quick {
            mode: ModeId(args.mode),
        },
    };
    let chair = Chair {
        addr: args.addr,
        login: args.login,
        password: args.password,
        want,
        heroes: heroes.clone(),
        seats: args.seats,
        limit: args.limit,
    };
    let trace = args.trace;
    let playbook = Rc::new(move |n: usize| {
        let mut playbook = Playbook::new(roles[n % roles.len()], heroes[n % heroes.len()]);
        playbook.trace_every(trace);
        playbook
    });
    // One playbook to a seat, however many seats the account holds.
    let crew = move || {
        let playbook = Rc::clone(&playbook);
        Squad::new(move |n, _| playbook(n))
    };
    let played: Vec<Outcome> = if let Want::Tournament { .. } = chair.want {
        play_every(move || Box::new(crew()) as Box<dyn Crew>, &chair)?
    } else {
        play_crew(&mut crew(), &chair)?
    };
    for out in &played {
        let mine = out.mine.as_ref();
        println!(
            "seat {:?} played {} ticks as {:?}: {} last hits, {} denies, {} kills, {} deaths, level {}",
            out.slot.map(|slot| slot.0),
            out.ticks,
            out.team,
            mine.map_or(0, |row| row.last_hits),
            mine.map_or(0, |row| row.denies),
            mine.map_or(0, |row| row.kills),
            mine.map_or(0, |row| row.deaths),
            mine.map_or(0, |row| row.level),
        );
        match out.winner {
            Some(winner) => println!("{winner:?} won"),
            None => println!("the match did not end"),
        }
        println!("{} orders sent, {} refused", out.ordered, out.rejected);
        for (reason, many) in &out.refusals {
            println!("  {many} refused: {reason:?}");
        }
    }
    Ok(())
}
