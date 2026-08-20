//! Teaching the network, in two halves.
//!
//! **Copying.** The rule-driven bot already plays a respectable lane, and
//! every order it gives is an answer to a question the network will be asked.
//! So the first half is not a search: play matches, write down what was shown
//! and what was chosen, and move the weights until the network chooses the
//! same. This is cheap — thousands of answers a match, and every one of them
//! carries a gradient — and it ends with a network that plays about as well as
//! what it copied. Starting a search there instead of at random is the single
//! largest thing that can be done for it.
//!
//! **Practising.** Copying cannot beat what it copied. The second half plays
//! the network against a frozen copy of itself, one side choosing loosely and
//! the other taking what it already believes, and moves the weights towards
//! the loose choices of the matches where wandering paid.
//!
//! The frozen side is the whole trick. Scoring a seat against the run of the
//! generation sounds reasonable and is nearly noise: both seats play the same
//! weights, so half of them come out above average whatever they did, and
//! sharpening towards those halves sharpens randomness. Measured against a
//! side that made no unusual choices on the same seed, the difference is what
//! the unusual choices were worth, which is the thing being asked. The first
//! cut of this had no frozen side and it walked the network downhill: the loss
//! fell while the matches got worse, which is what a policy agreeing with
//! itself ever harder looks like.
//!
//! Credit inside a match is handed out bluntly: every decision shares the
//! match's score. Nothing in the match says which tick won it, and inventing a
//! signal that is not there would be worse than admitting there is none.

use std::path::PathBuf;
use std::thread;

use crate::{
    Adam, Brain, Copying, Dice, Ground, Net, NetBrain, Outcome, Params, Recording, Worth,
    bout_between, choice_loss, score,
};

/// How a network is taught.
#[derive(Clone, Debug)]
pub struct School {
    /// Where the matches are played.
    pub ground: Ground,
    /// Matches played to gather one round of answers.
    pub matches: usize,
    /// How many of them run at once.
    pub lanes: usize,
    /// Times the gathered answers are gone over.
    pub passes: usize,
    /// Frames whose losses are added up before the weights move.
    pub batch: usize,
    /// How far a step goes.
    pub rate: f32,
    /// How loosely choices are made while practising. Zero is greedy.
    pub heat: f32,
    /// Where the search draws from and the matches are seeded.
    pub seed: u64,
    /// Where the weights are kept.
    pub keep: Option<PathBuf>,
    /// What the outcome of a match is worth.
    pub worth: Worth,
}

impl Default for School {
    fn default() -> School {
        School {
            ground: Ground::default(),
            matches: 6,
            lanes: 6,
            passes: 2,
            batch: 32,
            rate: 1e-3,
            heat: 0.7,
            seed: 1,
            keep: None,
            worth: Worth::default(),
        }
    }
}

/// What one round of teaching came to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lesson {
    /// Decisions gathered.
    pub frames: usize,
    /// What part of the orders given were candidates at all. Only copying
    /// answers this; practising picks from the candidates by construction.
    pub covered: f32,
    /// The loss over the last pass, per decision.
    pub loss: f32,
    /// How often the network now picks what it was taught to pick.
    pub agreement: f32,
    /// What a seat's match was worth on average.
    pub worth: f32,
}

/// Plays matches of the rule-driven bot and writes down every choice it made.
pub fn watch_the_rules(how: &School, params: &Params) -> std::io::Result<(Recording, f32)> {
    let mut gathered = Recording::new();
    let mut covered = (0.0, 0u32);
    let seeds: Vec<u64> = (0..how.matches)
        .map(|at| how.seed.wrapping_mul(0x9e37_79b9).wrapping_add(at as u64))
        .collect();
    for lane in seeds.chunks(how.lanes.max(1)) {
        let played: Vec<std::io::Result<(Recording, Recording, f32)>> = thread::scope(|scope| {
            let running: Vec<_> = lane
                .iter()
                .map(|seed| {
                    let seed = *seed;
                    scope.spawn(move || {
                        let mut one = Brain::with(*params);
                        let mut other = Brain::with(*params);
                        let mut watched = Copying::new(&mut one, *params);
                        let mut theirs = Copying::new(&mut other, *params);
                        bout_between(&how.ground, seed, &mut watched, &mut theirs)?;
                        let seen = (watched.covered() + theirs.covered()) / 2.0;
                        Ok((watched.recording, theirs.recording, seen))
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
            let (mine, theirs, seen) = outcome?;
            gathered.frames.extend(mine.frames);
            gathered.frames.extend(theirs.frames);
            covered = (covered.0 + seen, covered.1 + 1);
        }
    }
    let seen = if covered.1 == 0 {
        1.0
    } else {
        covered.0 / covered.1 as f32
    };
    Ok((gathered, seen))
}

/// Plays the loose network against a frozen greedy copy of itself, and writes
/// down what the loose side chose along with what wandering was worth.
///
/// Each seed is played twice with the sides swapped, so that nothing learned
/// is really a fact about which end of the map a seat started at.
pub fn practise_matches(
    how: &School,
    params: &Params,
    weights: &[u8],
    round: u64,
) -> std::io::Result<(Recording, f32)> {
    let mut gathered = Recording::new();
    let mut steady = (0.0f32, 0u32);
    let seeds: Vec<u64> = (0..how.matches)
        .map(|at| {
            how.seed
                .wrapping_mul(0x9e37_79b9)
                .wrapping_add(round.wrapping_mul(1013))
                .wrapping_add(at as u64)
        })
        .collect();
    let jobs: Vec<(u64, bool)> = seeds
        .iter()
        .flat_map(|seed| [(*seed, false), (*seed, true)])
        .collect();
    for lane in jobs.chunks(how.lanes.max(1)) {
        let played: Vec<std::io::Result<(Recording, f32)>> = thread::scope(|scope| {
            let running: Vec<_> = lane
                .iter()
                .map(|(seed, second)| {
                    let (seed, second) = (*seed, *second);
                    scope.spawn(move || one_practice_match(how, params, weights, seed, second))
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
            let (recording, held) = outcome?;
            gathered.frames.extend(recording.frames);
            steady = (steady.0 + held, steady.1 + 1);
        }
    }
    let held = if steady.1 == 0 {
        0.0
    } else {
        steady.0 / steady.1 as f32
    };
    Ok((gathered, held))
}

/// One match: the network wandering on one side, the same network taking what
/// it believes on the other.
///
/// What comes back is what the wandering side chose, each frame carrying how
/// much better than the steady side its match came out, and what the steady
/// side scored, which is the one number that means the same from one round to
/// the next.
fn one_practice_match(
    how: &School,
    params: &Params,
    weights: &[u8],
    seed: u64,
    loose_goes_second: bool,
) -> std::io::Result<(Recording, f32)> {
    let hatch = |heat: f32, seed: u64| -> std::io::Result<NetBrain> {
        let net = Net::from_bytes(weights, how.seed).map_err(std::io::Error::other)?;
        let mut brain = NetBrain::loosely(net, *params, heat, seed);
        if heat > 0.0 {
            brain.watch();
        }
        Ok(brain)
    };
    let mut loose = hatch(how.heat, seed ^ 0x5bf0_3635)?;
    let mut steady = hatch(0.0, seed)?;
    let (wandering, held) = if loose_goes_second {
        let (theirs, mine) = bout_between(&how.ground, seed, &mut steady, &mut loose)?;
        (mine, theirs)
    } else {
        bout_between(&how.ground, seed, &mut loose, &mut steady)?
    };
    let paid = worth_of(&wandering, &how.worth) - worth_of(&held, &how.worth);
    let steady_worth = worth_of(&held, &how.worth);
    let Some(mut recording) = loose.recording else {
        return Ok((Recording::new(), steady_worth));
    };
    recording.worth_was(paid);
    Ok((recording, steady_worth))
}

/// What one seat's match was worth, and nothing at all for a seat that never
/// played.
fn worth_of(out: &Outcome, worth: &Worth) -> f32 {
    let had = score(out, worth);
    if had == f32::MIN { 0.0 } else { had }
}

/// Moves the weights towards the choices in a recording.
///
/// Every frame is weighted. Copying weighs them all the same; practising
/// weighs them by how the match went, so the same machinery does both.
pub fn learn_from(
    net: &Net,
    adam: &mut Adam,
    frames: &Recording,
    weights: &[f32],
    how: &School,
    dice: &mut Dice,
) -> Lesson {
    let mut order: Vec<usize> = (0..frames.len()).collect();
    let mut loss_total = 0.0;
    let mut counted = 0.0;
    for _ in 0..how.passes.max(1) {
        shuffle(&mut order, dice);
        for batch in order.chunks(how.batch.max(1)) {
            let mut summed: Option<candle_core::Tensor> = None;
            for at in batch {
                let frame = &frames.frames[*at];
                let weight = weights.get(*at).copied().unwrap_or(1.0);
                if weight == 0.0 || frame.rows.len() < 2 {
                    continue;
                }
                let Ok(rows) = net.tensor_of(&frame.rows) else {
                    continue;
                };
                let Ok(scores) = net.run(&rows) else { continue };
                let Ok(loss) = choice_loss(&scores, frame.chosen, weight) else {
                    continue;
                };
                summed = Some(match summed {
                    None => loss,
                    Some(had) => match &had + &loss {
                        Ok(both) => both,
                        Err(_) => had,
                    },
                });
            }
            let Some(loss) = summed else { continue };
            if let Ok(number) = loss.to_vec1::<f32>() {
                loss_total += number.first().copied().unwrap_or(0.0);
                counted += batch.len() as f32;
            }
            let _ = adam.step(net, &loss);
        }
    }
    Lesson {
        frames: frames.len(),
        covered: 1.0,
        loss: if counted > 0.0 {
            loss_total / counted
        } else {
            0.0
        },
        agreement: agreement_of(net, frames),
        worth: average_worth(frames),
    }
}

/// How often the network picks what the recording says was picked.
pub fn agreement_of(net: &Net, frames: &Recording) -> f32 {
    let mut agreed = 0u32;
    let mut seen = 0u32;
    for frame in &frames.frames {
        let Ok(scores) = net.scores(&frame.rows) else {
            continue;
        };
        seen += 1;
        if crate::best_of(&scores) == frame.chosen {
            agreed += 1;
        }
    }
    if seen == 0 {
        0.0
    } else {
        agreed as f32 / seen as f32
    }
}

/// What the matches behind a recording were worth on average.
fn average_worth(frames: &Recording) -> f32 {
    if frames.is_empty() {
        return 0.0;
    }
    frames.frames.iter().map(|frame| frame.worth).sum::<f32>() / frames.len() as f32
}

/// How much each frame counts, given what wandering was worth in its match.
///
/// A match where wandering paid pulls its choices towards being made again, in
/// proportion to how much it paid; one where it did not counts for nothing.
/// Weighing the bad ones negatively was considered and left out: pushing away
/// from a choice says nothing about what to do instead, and there is always
/// another legal choice to fall into.
pub fn weigh_by_worth(frames: &Recording, sharpness: f32) -> Vec<f32> {
    if frames.is_empty() {
        return Vec::new();
    }
    let edges: Vec<f32> = frames.frames.iter().map(|frame| frame.worth).collect();
    let spread = {
        let sum: f32 = edges.iter().map(|edge| edge * edge).sum();
        (sum / edges.len() as f32).sqrt().max(1e-3)
    };
    edges
        .iter()
        .map(|edge| {
            if *edge <= 0.0 {
                0.0
            } else {
                (edge / spread * sharpness).min(4.0)
            }
        })
        .collect()
}

/// Keeps a recording down to a size worth going over.
///
/// A round gathers far more decisions than a pass over them needs, and every
/// one of them costs a matmul. Taken at random rather than from the front: the
/// first minutes of a match look nothing like the rest of it.
pub fn thin_to(frames: &mut Recording, most: usize, dice: &mut Dice) {
    if frames.len() <= most || most == 0 {
        return;
    }
    let mut order: Vec<usize> = (0..frames.len()).collect();
    shuffle(&mut order, dice);
    order.truncate(most);
    order.sort_unstable();
    let mut kept = Vec::with_capacity(most);
    for at in order {
        kept.push(frames.frames[at].clone());
    }
    frames.frames = kept;
}

/// Shuffles in place, from a stream that can be started again.
fn shuffle(order: &mut [usize], dice: &mut Dice) {
    for at in (1..order.len()).rev() {
        order.swap(at, dice.below(at + 1));
    }
}

/// How far ahead of the rule-driven bot the network is, over a fixed set of
/// matches.
///
/// The one number in a run that means the same thing from the first round to
/// the last. What a round's own matches were worth does not: every round plays
/// different seeds, so that number moves with the luck of the draw as much as
/// with the weights, and reading it as progress is reading noise. This plays
/// the same seeds every time, against an opponent that never changes, with
/// both sides taking what they believe rather than wandering.
pub fn measure_against_rules(
    how: &School,
    params: &Params,
    weights: &[u8],
) -> std::io::Result<f32> {
    let seeds: [u64; 2] = [0x5eed_0001, 0x5eed_0002];
    let jobs: Vec<(u64, bool)> = seeds
        .iter()
        .flat_map(|seed| [(*seed, false), (*seed, true)])
        .collect();
    let mut edge = 0.0;
    for lane in jobs.chunks(how.lanes.max(1)) {
        let played: Vec<std::io::Result<f32>> = thread::scope(|scope| {
            let running: Vec<_> = lane
                .iter()
                .map(|(seed, second)| {
                    let (seed, second) = (*seed, *second);
                    scope.spawn(move || {
                        let net =
                            Net::from_bytes(weights, how.seed).map_err(std::io::Error::other)?;
                        let mut learned = NetBrain::new(net, *params);
                        let mut rules = Brain::with(*params);
                        let (mine, theirs) = if second {
                            let (theirs, mine) =
                                bout_between(&how.ground, seed, &mut rules, &mut learned)?;
                            (mine, theirs)
                        } else {
                            bout_between(&how.ground, seed, &mut learned, &mut rules)?
                        };
                        Ok(worth_of(&mine, &how.worth) - worth_of(&theirs, &how.worth))
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
            edge += outcome?;
        }
    }
    Ok(edge / jobs.len() as f32)
}
