//! Command line entry point of the bot.
//!
//! Four things it can be asked to do: play a match, search the numbers the
//! rule-driven bot plays by, teach a network to choose what that bot chooses,
//! and let a network practise against itself.

use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};

use bota_bot::{
    Adam, Bot, Brain, Dice, Ground, Net, NetBrain, Params, Practice, School, Seat, Watched,
    journal_heading, learn_from, measure_against_rules, play, practice, practise_matches, thin_to,
    watch_the_rules, weigh_by_worth,
};
use bota_proto::HeroId;

/// A bot that plays a hero, and what it takes to train one.
///
/// The numbers and the weights a trained bot plays by sit beside the
/// repository and are read when it runs, so a training run takes effect
/// without a rebuild. Neither is committed: they are what one machine's
/// training arrived at.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    /// What to do. Playing, when nothing is said.
    #[command(subcommand)]
    doing: Option<Doing>,
    /// What playing takes, for when nothing is said.
    #[command(flatten)]
    playing: Playing,
}

/// The things the bot can be asked to do.
#[derive(Subcommand, Debug)]
enum Doing {
    /// Join a server and play one match.
    Play(Playing),
    /// Search over the numbers the rule-driven bot plays by.
    Train(Training),
    /// Teach a network to choose what the rule-driven bot chooses.
    Copy(Teaching),
    /// Let a network play itself and keep what it learns.
    Practise(Teaching),
}

/// Which numbers a run plays by.
#[derive(Args, Debug, Clone)]
struct Numbers {
    /// Numbers to play by. The kept ones, and the plain ones when none are
    /// kept.
    #[arg(long, value_name = "FILE")]
    params: Option<PathBuf>,
    /// Play by the numbers the code was written with.
    #[arg(long, conflicts_with = "params")]
    plain: bool,
}

impl Numbers {
    /// Reads them in.
    fn read(&self) -> std::io::Result<Params> {
        if self.plain {
            return Ok(Params::default());
        }
        let Some(path) = self.params.as_ref() else {
            return Ok(Params::learned());
        };
        let text = std::fs::read_to_string(path)?;
        Params::parse(&text).map_err(std::io::Error::other)
    }
}

/// Joining a server and playing.
#[derive(Args, Debug)]
struct Playing {
    /// Where the server listens.
    #[arg(long, default_value = "127.0.0.1:4455")]
    addr: String,
    /// What the lobby shows.
    #[arg(long, default_value = "bot")]
    name: String,
    /// Which hero to ask for.
    #[arg(long, default_value_t = 0)]
    hero: u16,
    /// Leave after this many ticks.
    #[arg(long, value_name = "TICKS")]
    limit: Option<u32>,
    /// Write a line a tick about what it saw and did.
    #[arg(long, value_name = "FILE")]
    trace: Option<PathBuf>,
    /// Play by the network rather than by the rules.
    #[arg(long)]
    net: bool,
    /// Which network to play by.
    #[arg(long, value_name = "FILE", default_value_os_t = Net::path())]
    weights: PathBuf,
    #[command(flatten)]
    numbers: Numbers,
}

/// Where matches are played, for anything that plays them.
#[derive(Args, Debug, Clone)]
struct Field {
    /// The server to run. The one built beside this, when nothing is said.
    #[arg(long, value_name = "PATH")]
    server: Option<PathBuf>,
    /// Which map.
    #[arg(long, default_value_t = 0)]
    map: u16,
    /// Which hero both seats play.
    #[arg(long, default_value_t = 0)]
    hero: u16,
    /// Ticks one match is played for.
    #[arg(long, default_value_t = 9000)]
    ticks: u32,
    /// Ticks a second, which in lockstep only sets the straggler timeout.
    #[arg(long, default_value_t = 30)]
    tick_rate: u16,
    /// Matches played at once.
    #[arg(long, default_value_t = 8)]
    lanes: usize,
    /// Where the search and the matches are seeded from.
    #[arg(long, default_value_t = 1)]
    seed: u64,
}

impl Field {
    /// The ground these matches are played on.
    fn ground(&self) -> Ground {
        let standing = Ground::default();
        Ground {
            server: self.server.clone().unwrap_or(standing.server),
            map: self.map,
            hero: HeroId(self.hero),
            ticks: self.ticks,
            tick_rate: self.tick_rate,
            ..standing
        }
    }
}

/// Searching over the numbers.
#[derive(Args, Debug)]
struct Training {
    /// Rounds to run.
    #[arg(long, default_value_t = 20)]
    rounds: u32,
    /// Challengers a round breeds.
    #[arg(long, default_value_t = 4)]
    challengers: usize,
    /// Numbers one challenger differs in.
    #[arg(long, default_value_t = 3)]
    nudges: usize,
    /// How far one nudge moves a number to begin with.
    #[arg(long, default_value_t = 0.15)]
    reach: f32,
    /// Rounds the champion stands for before the best set replaces it. Zero
    /// measures the whole run against the numbers the code was written with.
    #[arg(long, default_value_t = 25)]
    champion_every: u32,
    /// Where to write the numbers that play best.
    #[arg(long, value_name = "FILE", default_value_os_t = Params::path())]
    keep: PathBuf,
    /// Where to write a line about every round.
    #[arg(long, value_name = "FILE")]
    journal: Option<PathBuf>,
    #[command(flatten)]
    field: Field,
    #[command(flatten)]
    numbers: Numbers,
}

/// Teaching a network, either half of it.
#[derive(Args, Debug)]
struct Teaching {
    /// Where the network is kept.
    #[arg(long, value_name = "FILE", default_value_os_t = Net::path())]
    weights: PathBuf,
    /// Matches gathered a round.
    #[arg(long, default_value_t = 6)]
    matches: usize,
    /// Times the gathered decisions are gone over.
    #[arg(long, default_value_t = 2)]
    passes: usize,
    /// Decisions added up before the weights move.
    #[arg(long, default_value_t = 32)]
    batch: usize,
    /// How far a step goes.
    #[arg(long, default_value_t = 1e-3)]
    rate: f32,
    /// How loosely it chooses while practising. Zero is greedy.
    #[arg(long, default_value_t = 0.7)]
    heat: f32,
    /// Rounds of practising.
    #[arg(long, default_value_t = 20)]
    rounds: u32,
    #[command(flatten)]
    field: Field,
    #[command(flatten)]
    numbers: Numbers,
}

impl Teaching {
    /// The school these lessons are run in.
    fn school(&self) -> School {
        School {
            ground: self.field.ground(),
            matches: self.matches,
            lanes: self.field.lanes,
            passes: self.passes,
            batch: self.batch,
            rate: self.rate,
            heat: self.heat,
            seed: self.field.seed,
            keep: Some(self.weights.clone()),
            ..School::default()
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let doing = cli.doing.unwrap_or(Doing::Play(cli.playing));
    if let Err(err) = carry_out(doing) {
        eprintln!("bot: {err}");
        std::process::exit(1);
    }
}

/// Does what was asked for.
fn carry_out(doing: Doing) -> std::io::Result<()> {
    match doing {
        Doing::Play(asked) => join_a_match(asked),
        Doing::Train(asked) => search_the_numbers(asked),
        Doing::Copy(asked) => copy_the_rules(asked),
        Doing::Practise(asked) => practise_the_net(asked),
    }
}

/// Joins a server and plays one match.
fn join_a_match(asked: Playing) -> std::io::Result<()> {
    let params = asked.numbers.read()?;
    let seat = Seat {
        addr: asked.addr,
        name: asked.name,
        hero: HeroId(asked.hero),
        limit: asked.limit,
    };
    let mut rules = Brain::with(params);
    let mut learned = if asked.net {
        if !asked.weights.exists() {
            return Err(std::io::Error::other(format!(
                "no weights at {}: teach some with `bota-bot copy` first",
                asked.weights.display()
            )));
        }
        Some(NetBrain::new(
            Net::from_file(&asked.weights, 1).map_err(std::io::Error::other)?,
            params,
        ))
    } else {
        None
    };
    let bot: &mut dyn Bot = match learned.as_mut() {
        Some(net) => net,
        None => &mut rules,
    };
    let out = match asked.trace {
        None => play(bot, &seat)?,
        Some(path) => {
            let mut watched = Watched::writing_to(bot, &path)?;
            play(&mut watched, &seat)?
        }
    };
    let mine = out.mine.as_ref();
    println!(
        "played {} ticks: {} last hits, {} denies, {} kills, {} deaths",
        out.ticks,
        mine.map_or(0, |row| row.last_hits),
        mine.map_or(0, |row| row.denies),
        mine.map_or(0, |row| row.kills),
        mine.map_or(0, |row| row.deaths),
    );
    println!(
        "damage: {} to heroes, {} to what they built, {} taken",
        out.hero_damage, out.structure_damage, out.damage_taken,
    );
    Ok(())
}

/// Searches over the numbers the rule-driven bot plays by.
fn search_the_numbers(asked: Training) -> std::io::Result<()> {
    let how = Practice {
        ground: asked.field.ground(),
        rounds: asked.rounds,
        seed: asked.field.seed,
        reach: asked.reach,
        nudges: asked.nudges,
        challengers: asked.challengers,
        lanes: asked.field.lanes,
        champion_every: asked.champion_every,
        keep: Some(asked.keep.clone()),
        journal: asked.journal.clone(),
        ..Practice::default()
    };
    if let Some(path) = how.journal.as_ref()
        && !path.exists()
    {
        std::fs::write(path, format!("{}\n", journal_heading()))?;
    }
    println!("keeping the best numbers in {}", asked.keep.display());
    let best = practice(asked.numbers.read()?, &how)?;
    print!("{}", best.to_text());
    Ok(())
}

/// Teaches a network to choose what the rule-driven bot chooses.
fn copy_the_rules(asked: Teaching) -> std::io::Result<()> {
    let how = asked.school();
    let params = asked.numbers.read()?;
    let net = open_the_net(&asked.weights, how.seed)?;
    let mut adam = Adam::new(&net, how.rate).map_err(std::io::Error::other)?;
    let mut dice = Dice::from_seed(how.seed ^ 0x51ed_2701);
    println!("network of {} weights", net.weight_count());
    let (frames, covered) = watch_the_rules(&how, &params)?;
    println!(
        "watched {} decisions, {:.1}% of them candidates the network could pick",
        frames.len(),
        covered * 100.0
    );
    let all_the_same = vec![1.0; frames.len()];
    let lesson = learn_from(&net, &mut adam, &frames, &all_the_same, &how, &mut dice);
    println!(
        "loss {:.3}, agreeing with the rules {:.1}% of the time",
        lesson.loss,
        lesson.agreement * 100.0
    );
    net.save(&asked.weights).map_err(std::io::Error::other)?;
    println!("kept the weights in {}", asked.weights.display());
    Ok(())
}

/// Lets a network wander against a steady copy of itself, and moves it towards
/// what wandering was worth.
///
/// Some of what it copied is kept back and gone over again every round. A
/// policy taught only from its own recent matches forgets the parts of the
/// game those matches did not visit, and there is nothing in a lane to remind
/// it.
fn practise_the_net(asked: Teaching) -> std::io::Result<()> {
    let how = asked.school();
    let params = asked.numbers.read()?;
    let weights = asked.weights.clone();
    let net = open_the_net(&weights, how.seed)?;
    let mut adam = Adam::new(&net, how.rate).map_err(std::io::Error::other)?;
    let mut dice = Dice::from_seed(how.seed ^ 0x2545_f491);
    println!("gathering what it was taught, to go over again as it practises");
    let (mut rehearse, _) = watch_the_rules(&how, &params)?;
    thin_to(&mut rehearse, REHEARSED, &mut dice);
    // What the weights were worth before any of this. A run that never beats
    // it leaves the file exactly as it found it.
    let working = weights.with_extension("practising");
    net.save(&working).map_err(std::io::Error::other)?;
    let mut best = measure_against_rules(&how, &params, &std::fs::read(&working)?)?;
    println!("starting out ahead of the rules by {best:.1}");
    for round in 1..=asked.rounds {
        net.save(&working).map_err(std::io::Error::other)?;
        let bytes = std::fs::read(&working)?;
        let (mut frames, steady) = practise_matches(&how, &params, &bytes, u64::from(round))?;
        thin_to(&mut frames, GATHERED, &mut dice);
        let mut weighed = weigh_by_worth(&frames, 1.0);
        let paid = weighed.iter().filter(|weight| **weight > 0.0).count();
        // What it was taught counts for something on every round, so that a
        // run of odd matches cannot talk it out of the whole game.
        frames.frames.extend(rehearse.frames.iter().cloned());
        weighed.extend(std::iter::repeat_n(REHEARSAL_WEIGHT, rehearse.len()));
        let lesson = learn_from(&net, &mut adam, &frames, &weighed, &how, &mut dice);
        net.save(&working).map_err(std::io::Error::other)?;
        let mut kept = String::new();
        if round.is_multiple_of(MEASURE_EVERY) || round == asked.rounds {
            let edge = measure_against_rules(&how, &params, &std::fs::read(&working)?)?;
            // Only what measures better is kept. Practising wanders as much as
            // it climbs — it went twenty-six points ahead by the tenth round of
            // one run and fifteen behind by the thirtieth — and a run that
            // writes out its last weights rather than its best throws away the
            // good ones on the way past.
            kept = if edge > best {
                best = edge;
                net.save(&weights).map_err(std::io::Error::other)?;
                format!(", ahead by {edge:.1}, kept")
            } else {
                format!(", ahead by {edge:.1}, best still {best:.1}")
            };
        }
        println!(
            "round {round}: this round's matches worth {steady:.1}, {paid} of {} \
             wandering decisions paid, loss {:.3}{kept}",
            lesson.frames - rehearse.len(),
            lesson.loss
        );
    }
    let _ = std::fs::remove_file(&working);
    println!("the best it managed was {best:.1} ahead of the rules");
    Ok(())
}

/// Rounds between measuring against something that does not move.
const MEASURE_EVERY: u32 = 5;
/// Decisions kept back from what was copied, to go over again each round.
const REHEARSED: usize = 6000;
/// How much one of those counts against a decision that paid.
const REHEARSAL_WEIGHT: f32 = 0.3;
/// Decisions of one round's wandering that are gone over.
const GATHERED: usize = 12000;

/// The network a file holds, or a fresh one when it holds none.
fn open_the_net(weights: &Path, seed: u64) -> std::io::Result<Net> {
    if weights.exists() {
        Net::from_file(weights, seed).map_err(std::io::Error::other)
    } else {
        println!("no weights at {}, starting fresh", weights.display());
        Net::fresh(seed).map_err(std::io::Error::other)
    }
}
