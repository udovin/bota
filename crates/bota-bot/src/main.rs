//! Command line entry point of the bot.

use bota_bot::{Chair, Playbook, Role, Want, play, play_every};
use bota_proto::{HeroId, MatchId, ModeId, TournamentId};
use clap::Parser;

/// bota bot: signs in, takes a seat, and plays.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Where the server listens.
    #[arg(long, default_value = "127.0.0.1:4455")]
    addr: String,
    /// The account to play as. Registered the first time it is used.
    #[arg(long, default_value = "bot")]
    login: String,
    /// Its password.
    #[arg(long, default_value = "bot")]
    password: String,
    /// Take a seat in any open match of this mode.
    #[arg(long, default_value_t = 0, conflicts_with_all = ["match_id", "tournament"])]
    mode: u16,
    /// Take a seat in this match instead.
    #[arg(long = "match", value_name = "ID")]
    match_id: Option<u64>,
    /// Play every game this tournament gives.
    #[arg(long, value_name = "ID", requires = "team")]
    tournament: Option<u64>,
    /// Which team to register in the tournament as.
    #[arg(long)]
    team: Option<String>,
    /// Which hero to ask for: 0 Sylla, 1 Pudge, 2 Shadow Fiend.
    #[arg(long, default_value_t = 2)]
    hero: u16,
    /// What the seat is there to do, one to five.
    #[arg(long, default_value_t = 2)]
    role: u8,
    /// Leave after this many ticks.
    #[arg(long, value_name = "TICKS")]
    limit: Option<u32>,
    /// Write a line about what the seat is doing every so many ticks.
    #[arg(long, value_name = "TICKS", default_value_t = 0)]
    trace: u32,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let Some(role) = Role::of(args.role) else {
        return Err(std::io::Error::other(format!(
            "there is no role {}: they are numbered one to five",
            args.role
        )));
    };
    let hero = HeroId(args.hero);
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
        hero,
        limit: args.limit,
    };
    let trace = args.trace;
    let make = move || {
        let mut playbook = Playbook::new(role, hero);
        playbook.trace_every(trace);
        Box::new(playbook) as Box<dyn bota_bot::Bot>
    };
    let played = match chair.want {
        Want::Tournament { .. } => play_every(make, &chair)?,
        _ => {
            let mut bot = make();
            vec![play(bot.as_mut(), &chair)?]
        }
    };
    for out in &played {
        let mine = out.mine.as_ref();
        println!(
            "played {} ticks as {:?}: {} last hits, {} denies, {} kills, {} deaths, level {}",
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
