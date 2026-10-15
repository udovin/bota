//! Pairing teams and moving a tournament along, apart from any match.

use bota_proto::{GameInfo, PairInfo, RoundInfo, TournamentFormat, TournamentInfo};

/// The rounds of a tournament over `teams` teams, before any game.
///
/// Single elimination seeds the first round first against last and leaves
/// a bye where the bracket has no team; later rounds wait to be filled.
/// Round robin pairs everybody with everybody once, one round per turn of
/// the circle.
pub fn pair_up(format: TournamentFormat, teams: u8) -> Vec<RoundInfo> {
    match format {
        TournamentFormat::SingleElimination => elimination_rounds(teams),
        TournamentFormat::RoundRobin => robin_rounds(teams),
    }
}

fn elimination_rounds(teams: u8) -> Vec<RoundInfo> {
    let size = (teams as usize).max(2).next_power_of_two();
    let seed_at = |i: usize| -> Option<u8> { (i < usize::from(teams)).then_some(i as u8) };
    let mut rounds = Vec::new();
    let mut pairs: Vec<PairInfo> = (0..size / 2)
        .map(|i| PairInfo {
            a: seed_at(i),
            b: seed_at(size - 1 - i),
            games: Vec::new(),
            winner: None,
        })
        .collect();
    for pair in &mut pairs {
        if pair.b.is_none() {
            pair.winner = pair.a;
        }
    }
    rounds.push(RoundInfo { pairs });
    let mut width = size / 2;
    while width > 1 {
        width /= 2;
        rounds.push(RoundInfo {
            pairs: (0..width)
                .map(|_| PairInfo {
                    a: None,
                    b: None,
                    games: Vec::new(),
                    winner: None,
                })
                .collect(),
        });
    }
    rounds
}

fn robin_rounds(teams: u8) -> Vec<RoundInfo> {
    let n = usize::from(teams);
    let size = if n % 2 == 1 { n + 1 } else { n };
    let mut ring: Vec<Option<u8>> = (0..size).map(|i| (i < n).then_some(i as u8)).collect();
    let mut rounds = Vec::new();
    for _ in 0..size.saturating_sub(1) {
        let pairs = (0..size / 2)
            .map(|i| {
                let a = ring[i];
                let b = ring[size - 1 - i];
                PairInfo {
                    a,
                    b,
                    games: Vec::new(),
                    winner: if a.is_none() || b.is_none() {
                        a.or(b)
                    } else {
                        None
                    },
                }
            })
            .collect();
        rounds.push(RoundInfo { pairs });
        // The first stays; the rest turn one step.
        let last = ring.pop().expect("the ring is not empty");
        ring.insert(1, last);
    }
    rounds
}

/// The round being played: the first with a pair still undecided.
pub fn current_round(info: &TournamentInfo) -> Option<usize> {
    info.rounds
        .iter()
        .position(|round| round.pairs.iter().any(|pair| pair.winner.is_none()))
}

/// Games a team must win to take a series.
pub fn wins_needed(best_of: u8) -> u8 {
    best_of / 2 + 1
}

/// Records the result of a game and settles the series and the bracket
/// where it can. Returns whether the tournament is over.
pub fn score(info: &mut TournamentInfo, round: usize, pair: usize, game: GameInfo) -> bool {
    let needed = wins_needed(info.best_of);
    let Some(entry) = info
        .rounds
        .get_mut(round)
        .and_then(|r| r.pairs.get_mut(pair))
    else {
        return false;
    };
    match entry.games.iter_mut().find(|g| g.winner.is_none()) {
        Some(open) => *open = game,
        None => entry.games.push(game),
    }
    let (a, b) = (entry.a, entry.b);
    let won = |team: Option<u8>| {
        entry
            .games
            .iter()
            .filter(|g| g.winner == team && team.is_some())
            .count()
    };
    if won(a) >= usize::from(needed) {
        entry.winner = a;
    } else if won(b) >= usize::from(needed) {
        entry.winner = b;
    }
    if entry.winner.is_none() {
        return false;
    }
    let winner = entry.winner;
    if info.format == TournamentFormat::SingleElimination
        && let Some(next) = info.rounds.get_mut(round + 1)
        && let Some(slot) = next.pairs.get_mut(pair / 2)
    {
        if pair.is_multiple_of(2) {
            slot.a = winner;
        } else {
            slot.b = winner;
        }
    }
    settle(info)
}

/// Marks the tournament finished once nothing is left to play. Returns
/// whether it is.
pub fn settle(info: &mut TournamentInfo) -> bool {
    if current_round(info).is_some() {
        return false;
    }
    info.winner = match info.format {
        TournamentFormat::SingleElimination => info
            .rounds
            .last()
            .and_then(|round| round.pairs.first())
            .and_then(|pair| pair.winner),
        TournamentFormat::RoundRobin => standings(info).first().copied(),
    };
    info.stage = bota_proto::TournamentStage::Finished;
    true
}

/// Teams best first: series won, then games won, then registration order.
pub fn standings(info: &TournamentInfo) -> Vec<u8> {
    let mut rows: Vec<(u8, usize, usize)> = (0..info.teams.len() as u8)
        .map(|team| {
            let series = info
                .rounds
                .iter()
                .flat_map(|r| &r.pairs)
                .filter(|p| p.winner == Some(team) && p.a.is_some() && p.b.is_some())
                .count();
            let games = info
                .rounds
                .iter()
                .flat_map(|r| &r.pairs)
                .flat_map(|p| &p.games)
                .filter(|g| g.winner == Some(team))
                .count();
            (team, series, games)
        })
        .collect();
    rows.sort_by(|x, y| y.1.cmp(&x.1).then(y.2.cmp(&x.2)).then(x.0.cmp(&y.0)));
    rows.into_iter().map(|row| row.0).collect()
}

/// Pairs of the current round whose next game may be created: both teams
/// known, the series open, and no game of it in play.
pub fn playable(info: &TournamentInfo) -> Vec<(usize, usize)> {
    let Some(round) = current_round(info) else {
        return Vec::new();
    };
    info.rounds[round]
        .pairs
        .iter()
        .enumerate()
        .filter(|(_, pair)| {
            pair.winner.is_none()
                && pair.a.is_some()
                && pair.b.is_some()
                && pair.games.iter().all(|g| g.winner.is_some())
        })
        .map(|(i, _)| (round, i))
        .collect()
}
