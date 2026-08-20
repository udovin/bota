//! Learning by playing: one set of numbers measured against another, over and
//! over.
//!
//! A round breeds several challengers out of the set in hand, each differing in
//! a few numbers, and measures every one of them the same way: two matches
//! against a champion, one from each side. Whichever came out furthest ahead of
//! the champion takes the set in hand, if it came out further ahead than that
//! set did. Both sides are played because the map is not symmetric to a search:
//! left to one side it would learn the side rather than the game.
//!
//! Measuring against a champion rather than against the set in hand is what
//! makes a round mean something. Head to head, the thing being climbed moves
//! under the search every time it takes a step: a challenger that beat the set
//! in hand this round says nothing about the round before, and a run of them
//! walks rather than climbs. Against something frozen, better is one number
//! that means the same in the first round and the hundredth.
//!
//! The champion is frozen, not fixed. It starts as the numbers the code was
//! written with and is replaced by the best set every so often, so that the bar
//! rises: a search would otherwise spend its whole run learning to beat a bot
//! it had already beaten.
//!
//! The matches of a round are all independent, so they are played at once,
//! which is what a round costs the wall clock rather than the sum of them.
//!
//! How far a nudge reaches is not fixed either. It widens while more than a
//! fifth of the challengers beat the set in hand and narrows while fewer do: a
//! search that keeps failing is reaching too far, and one that nearly always
//! succeeds is not reaching far enough. A fifth is the old rule of thumb for
//! this, and what matters about it is that both the widening and the narrowing
//! actually happen: widening on any success at all only ever widens.
//!
//! Every round is written down. The file of numbers that played best is
//! rewritten whenever it changes, so a run that is stopped halfway leaves
//! something to start again from.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::thread;

use crate::{Dice, Ground, Outcome, Params, Worth, bout, score};

/// How far a nudge may reach, at the widest and at the narrowest.
const REACH_LIMITS: (f32, f32) = (0.004, 0.5);
/// The share of challengers that should be beating the set in hand.
const WANTED_SHARE: f32 = 0.2;
/// What a round above that share does to the reach.
const WIDEN: f32 = 1.3;
/// What a round below it does.
const NARROW: f32 = 0.88;

/// How a search is run.
#[derive(Clone, Debug)]
pub struct Practice {
    /// Where the matches are played.
    pub ground: Ground,
    /// Rounds to run.
    pub rounds: u32,
    /// Where the search draws from, and where the matches are seeded from.
    pub seed: u64,
    /// How far one nudge moves a number to begin with, as a part of the range
    /// it may take. The search moves this itself as it goes.
    pub reach: f32,
    /// How many numbers one challenger differs in.
    pub nudges: usize,
    /// How many challengers a round breeds.
    pub challengers: usize,
    /// How many matches are played at once.
    pub lanes: usize,
    /// Rounds the champion stands for before the best set replaces it.
    ///
    /// Zero never replaces it, so the whole run is measured against the numbers
    /// the code was written with.
    pub champion_every: u32,
    /// Where to keep the numbers that played best.
    pub keep: Option<PathBuf>,
    /// Where to write a line about every round.
    pub journal: Option<PathBuf>,
    /// What the outcome of a match is worth.
    pub worth: Worth,
}

impl Default for Practice {
    fn default() -> Practice {
        Practice {
            ground: Ground::default(),
            rounds: 20,
            seed: 1,
            reach: 0.15,
            nudges: 3,
            challengers: 4,
            lanes: 8,
            champion_every: 25,
            keep: None,
            journal: None,
            worth: Worth::default(),
        }
    }
}

/// What one round came to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Round {
    /// Which round it was, counting from one.
    pub number: u32,
    /// How far ahead of the champion the set in hand came out.
    pub holder: f32,
    /// How far ahead of it the best challenger came out.
    pub challenger: f32,
    /// How far a nudge reached this round.
    pub reach: f32,
    /// Whether the challenger took its place.
    pub kept: bool,
    /// Whether the champion was replaced at the end of it.
    pub crowned: bool,
}

/// Runs the search and answers with the numbers that played best.
pub fn practice(start: Params, how: &Practice) -> std::io::Result<Params> {
    let mut dice = Dice::from_seed(how.seed);
    let mut best = start.clamped();
    let mut champion = Params::default();
    let mut reach = how.reach.clamp(REACH_LIMITS.0, REACH_LIMITS.1);
    write_the_numbers(how, &best)?;
    for number in 1..=how.rounds {
        let seed = how
            .seed
            .wrapping_mul(0x9e37_79b9)
            .wrapping_add(u64::from(number));
        // The set in hand is measured alongside the challengers, on the same
        // matches: the champion it is measured against changes, so what it was
        // worth last round is not what it is worth now.
        let mut sets = vec![best];
        sets.extend(
            (0..how.challengers.max(1)).map(|_| nudge(&best, &mut dice, how.nudges, reach)),
        );
        let ahead = measure_them_all(how, seed, &champion, &sets)?;
        let holder = ahead[0];
        let theirs = ahead[1..].iter().copied().fold(f32::MIN, f32::max);
        let at = ahead[1..]
            .iter()
            .position(|edge| *edge == theirs)
            .map_or(0, |at| at + 1);
        let kept = theirs > holder;
        if kept {
            best = sets[at];
            write_the_numbers(how, &best)?;
        }
        let beat_it = ahead[1..].iter().filter(|edge| **edge > holder).count();
        let crowned = how.champion_every > 0 && number.is_multiple_of(how.champion_every);
        if crowned {
            champion = best;
        }
        let round = Round {
            number,
            holder,
            challenger: theirs,
            reach,
            kept,
            crowned,
        };
        write_the_journal(how, &round, &sets[at])?;
        say_how_it_went(&round);
        let reaching_short = beat_it as f32 > WANTED_SHARE * (sets.len() - 1) as f32;
        reach = (if reaching_short {
            reach * WIDEN
        } else {
            reach * NARROW
        })
        .clamp(REACH_LIMITS.0, REACH_LIMITS.1);
    }
    Ok(best)
}

/// How far ahead of the champion each set comes out, over two matches apiece.
///
/// The matches are independent, so as many of them as there are lanes run at
/// once.
fn measure_them_all(
    how: &Practice,
    seed: u64,
    champion: &Params,
    sets: &[Params],
) -> std::io::Result<Vec<f32>> {
    let jobs: Vec<(usize, bool)> = (0..sets.len())
        .flat_map(|at| [(at, false), (at, true)])
        .collect();
    let mut ahead = vec![0.0; sets.len()];
    for lane in jobs.chunks(how.lanes.max(1)) {
        let played: Vec<std::io::Result<(usize, f32)>> = thread::scope(|scope| {
            let running: Vec<_> = lane
                .iter()
                .map(|(at, second)| {
                    let (at, second) = (*at, *second);
                    let theirs = sets[at];
                    scope.spawn(move || {
                        let (one, other) = if second {
                            bout(&how.ground, seed, *champion, theirs)?
                        } else {
                            bout(&how.ground, seed, theirs, *champion)?
                        };
                        let (mine, yours) = if second { (other, one) } else { (one, other) };
                        Ok((at, self_worth(how, &mine) - self_worth(how, &yours)))
                    })
                })
                .collect();
            running
                .into_iter()
                .map(|one| {
                    one.join()
                        .unwrap_or_else(|_| Err(std::io::Error::other("a match gave up")))
                })
                .collect()
        });
        for outcome in played {
            let (at, edge) = outcome?;
            ahead[at] += edge;
        }
    }
    Ok(ahead)
}

/// Says on the terminal how a round went.
fn say_how_it_went(round: &Round) {
    let Round {
        number,
        holder,
        challenger,
        reach,
        kept,
        crowned,
    } = round;
    let taken = if *kept { ", taken" } else { "" };
    let crowning = if *crowned { ", crowned" } else { "" };
    println!(
        "round {number}: ahead of the champion by {holder:.1}, \
         best challenger by {challenger:.1}{taken} (reach {reach:.3}){crowning}"
    );
}

/// What one seat's match was worth, and nothing at all for a seat that never
/// got to play.
fn self_worth(how: &Practice, out: &Outcome) -> f32 {
    let worth = score(out, &how.worth);
    if worth == f32::MIN { 0.0 } else { worth }
}

/// The set in hand with a few of its numbers moved.
pub fn nudge(from: &Params, dice: &mut Dice, nudges: usize, reach: f32) -> Params {
    let mut values = from.to_vec();
    let count = Params::count();
    for _ in 0..nudges.max(1) {
        let at = dice.below(count);
        let (low, high) = Params::RANGES[at];
        values[at] += dice.spread() * (high - low) * reach;
    }
    Params::from_slice(&values).clamped()
}

/// Writes the numbers that played best, when there is somewhere to write them.
///
/// Written beside and moved into place, because the file it replaces is one the
/// crate is built from: a build that catches it halfway through a plain write
/// reads half a set of numbers.
fn write_the_numbers(how: &Practice, params: &Params) -> std::io::Result<()> {
    let Some(path) = how.keep.as_ref() else {
        return Ok(());
    };
    let beside = path.with_extension("writing");
    fs::write(&beside, params.to_text())?;
    fs::rename(&beside, path)
}

/// Adds one line about a round, when there is a journal to add it to.
///
/// A line is the round, how far ahead of the champion each side came out,
/// whether the challenger was taken, and the numbers it played by: enough to
/// plot the run afterwards, and enough to start again from any round of it.
fn write_the_journal(how: &Practice, round: &Round, challenger: &Params) -> std::io::Result<()> {
    let Some(path) = how.journal.as_ref() else {
        return Ok(());
    };
    let mut line = format!(
        "{} {} {:.3} {:.3} {:.4} {}",
        round.number,
        if round.kept { "taken" } else { "left" },
        round.holder,
        round.challenger,
        round.reach,
        if round.crowned { "crowned" } else { "-" }
    );
    for value in challenger.to_vec() {
        line.push(' ');
        line.push_str(&value.to_string());
    }
    line.push('\n');
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())
}

/// The heading a journal is read with: what each column holds.
pub fn journal_heading() -> String {
    let mut line = String::from("# round kept holder_edge challenger_edge reach crowned");
    for name in Params::NAMES {
        line.push(' ');
        line.push_str(name);
    }
    line
}
