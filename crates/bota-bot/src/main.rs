//! Command line entry point of the bot: play one match, or train by playing
//! many.

use std::path::PathBuf;

use bota_bot::{Brain, Params, Practice, Seat, Watched, journal_heading, play, practice};
use bota_proto::HeroId;

/// What the command line asked for.
enum Asked {
    /// Join a server and play one match.
    Play(Seat, Numbers, Option<PathBuf>),
    /// Play matches against itself and keep what plays best.
    Train(Box<Practice>, Numbers),
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args
        .first()
        .is_some_and(|first| first == "help" || first == "--help")
    {
        println!("{USAGE}");
        return;
    }
    let training = args.first().is_some_and(|first| first == "train");
    if training || args.first().is_some_and(|first| first == "play") {
        args.remove(0);
    }
    let asked = match read_the_arguments(&args, training) {
        Ok(asked) => asked,
        Err(err) => {
            eprintln!("bot: {err}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    if let Err(err) = carry_out(asked) {
        eprintln!("bot: {err}");
        std::process::exit(1);
    }
}

/// Does what was asked for.
fn carry_out(asked: Asked) -> std::io::Result<()> {
    match asked {
        Asked::Play(seat, params, trail) => {
            let mut brain = Brain::with(params.read()?);
            let out = match trail {
                None => play(&mut brain, &seat)?,
                Some(path) => {
                    let mut watched = Watched::writing_to(&mut brain, &path)?;
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
        Asked::Train(how, params) => {
            if let Some(path) = how.journal.as_ref()
                && !path.exists()
            {
                std::fs::write(path, format!("{}\n", journal_heading()))?;
            }
            if let Some(path) = how.keep.as_ref() {
                println!("keeping the best numbers in {}", path.display());
            }
            let best = practice(params.read()?, &how)?;
            print!("{}", best.to_text());
            Ok(())
        }
    }
}

/// Which numbers a run plays by.
#[derive(Clone, Debug)]
enum Numbers {
    /// The ones the last training run left, as they were baked in when this
    /// was built.
    Learned,
    /// The ones the file in the source tree holds right now.
    ///
    /// What training starts from: it writes that file, so it reads it, and a
    /// second run carries on from where the first stopped without a rebuild in
    /// between.
    Kept,
    /// The ones the code was written with.
    Plain,
    /// The ones a file holds.
    From(PathBuf),
}

impl Numbers {
    /// Reads them in.
    fn read(&self) -> std::io::Result<Params> {
        match self {
            Numbers::Learned => Ok(Params::learned()),
            Numbers::Kept => match std::fs::read_to_string(Params::PATH) {
                Err(_) => Ok(Params::learned()),
                Ok(text) => Params::parse(&text).map_err(std::io::Error::other),
            },
            Numbers::Plain => Ok(Params::default()),
            Numbers::From(path) => {
                let text = std::fs::read_to_string(path)?;
                Params::parse(&text).map_err(std::io::Error::other)
            }
        }
    }
}

/// Reads the arguments into what they asked for.
fn read_the_arguments(args: &[String], training: bool) -> Result<Asked, String> {
    let mut seat = Seat {
        addr: "127.0.0.1:4455".to_string(),
        name: "bot".to_string(),
        hero: HeroId(0),
        limit: None,
    };
    let mut how = Practice {
        keep: Some(PathBuf::from(Params::PATH)),
        ..Practice::default()
    };
    let mut params = if training {
        Numbers::Kept
    } else {
        Numbers::Learned
    };
    let mut trail = None;
    let mut left = args.iter();
    while let Some(flag) = left.next() {
        let mut value = || {
            left.next()
                .cloned()
                .ok_or_else(|| format!("{flag} wants something after it"))
        };
        match flag.as_str() {
            "--addr" => seat.addr = value()?,
            "--name" => seat.name = value()?,
            "--hero" => {
                let hero = HeroId(number(&value()?)?);
                seat.hero = hero;
                how.ground.hero = hero;
            }
            "--params" => params = Numbers::From(PathBuf::from(value()?)),
            "--plain" => params = Numbers::Plain,
            "--trace" => trail = Some(PathBuf::from(value()?)),
            "--limit" => seat.limit = Some(number(&value()?)?),
            "--server" => how.ground.server = PathBuf::from(value()?),
            "--map" => how.ground.map = number(&value()?)?,
            "--ticks" => how.ground.ticks = number(&value()?)?,
            "--tick-rate" => how.ground.tick_rate = number(&value()?)?,
            "--rounds" => how.rounds = number(&value()?)?,
            "--seed" => how.seed = number(&value()?)?,
            "--reach" => how.reach = decimal(&value()?)?,
            "--nudges" => how.nudges = number(&value()?)?,
            "--challengers" => how.challengers = number(&value()?)?,
            "--lanes" => how.lanes = number(&value()?)?,
            "--champion-every" => how.champion_every = number(&value()?)?,
            "--keep" => how.keep = Some(PathBuf::from(value()?)),
            "--journal" => how.journal = Some(PathBuf::from(value()?)),
            other => return Err(format!("nothing is called {other}")),
        }
    }
    Ok(if training {
        Asked::Train(Box::new(how), params)
    } else {
        Asked::Play(seat, params, trail)
    })
}

/// One whole number from the command line.
fn number<T: std::str::FromStr>(text: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{text} is not a number here"))
}

/// One number with a fraction from the command line.
fn decimal(text: &str) -> Result<f32, String> {
    text.parse()
        .map_err(|_| format!("{text} is not a number here"))
}

/// What the command line takes.
const USAGE: &str = "\
bota-bot [play] [options]   join a server and play
bota-bot train [options]    play against itself and keep what plays best

play:
  --addr <host:port>   where the server listens (127.0.0.1:4455)
  --name <name>        what the lobby shows (bot)
  --hero <id>          which hero to ask for (0)
  --params <file>      numbers to play by (the ones training left)
  --plain              play by the numbers the code was written with
  --limit <ticks>      leave after this many ticks
  --trace <file>       write a line a tick about what it saw and did

train:
  --server <path>      the server to run (the one built beside this)
  --params <file>      numbers to start from (the kept file, then the baked set)
  --plain              start from the numbers the code was written with
  --keep <file>        where to write the best (the source tree's params.txt)
  --journal <file>     where to write a line about every round
  --rounds <n>         rounds to run (20)
  --challengers <n>    challengers a round breeds, two matches each (4)
  --lanes <n>          matches played at once (8)
  --champion-every <n> rounds the champion stands for before the best set
                       replaces it, zero to measure the whole run against the
                       numbers the code was written with (25)
  --ticks <n>          ticks one match is played for (9000)
  --seed <n>           where the search and the matches are seeded from (1)
  --map <id>           which map (0)
  --hero <id>          which hero both seats play (0)
  --reach <part>       how far one nudge moves a number to begin with (0.15)
  --nudges <n>         how many numbers a challenger differs in (3)
  --tick-rate <n>      ticks a second, which sets the straggler timeout (30)

The kept numbers are baked in when the bot is built, so a run that improves
them takes a rebuild before playing by them.";
