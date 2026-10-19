use bota_proto::{
    FromHub, GameInfo, HallReason, HeroId, ModeId, Role, SlotId, Stage, ToHub, TournamentFormat,
    TournamentInfo, TournamentStage,
};

use super::fixtures::*;
use crate::hub::bracket::{current_round, pair_up, playable, score, standings};

fn info(format: TournamentFormat, teams: u8, best_of: u8) -> TournamentInfo {
    TournamentInfo {
        id: bota_proto::TournamentId(1),
        name: "cup".to_string(),
        mode: ModeId(0),
        format,
        best_of,
        team_size: 1,
        organizer: "org".to_string(),
        stage: TournamentStage::Running,
        teams: (0..teams)
            .map(|i| bota_proto::TeamInfo {
                name: format!("t{i}"),
                members: vec![bota_proto::MemberInfo {
                    login: format!("p{i}"),
                    seats: 1,
                }],
            })
            .collect(),
        rounds: pair_up(format, teams),
        winner: None,
        created_at: 0,
    }
}

#[test]
fn single_elimination_seeds_first_against_last_and_byes_advance() {
    let rounds = pair_up(TournamentFormat::SingleElimination, 3);
    assert_eq!(rounds.len(), 2);
    assert_eq!(rounds[0].pairs.len(), 2);
    assert_eq!(
        (rounds[0].pairs[0].a, rounds[0].pairs[0].b),
        (Some(0), None)
    );
    assert_eq!(rounds[0].pairs[0].winner, Some(0), "a bye is a win");
    assert_eq!(
        (rounds[0].pairs[1].a, rounds[0].pairs[1].b),
        (Some(1), Some(2))
    );
    assert_eq!(rounds[1].pairs.len(), 1);
    assert_eq!(rounds[1].pairs[0].a, None, "waits for the first round");

    let five = pair_up(TournamentFormat::SingleElimination, 5);
    assert_eq!(five.len(), 3);
    assert_eq!(five[0].pairs.len(), 4);
    assert_eq!((five[0].pairs[0].a, five[0].pairs[0].b), (Some(0), None));
    assert_eq!((five[0].pairs[3].a, five[0].pairs[3].b), (Some(3), Some(4)));
}

#[test]
fn round_robin_meets_everybody_once() {
    for teams in [2u8, 3, 4, 5, 6] {
        let rounds = pair_up(TournamentFormat::RoundRobin, teams);
        let mut met = Vec::new();
        for round in &rounds {
            let mut seen = Vec::new();
            for pair in &round.pairs {
                if let (Some(a), Some(b)) = (pair.a, pair.b) {
                    assert!(!seen.contains(&a) && !seen.contains(&b), "one game a round");
                    seen.push(a);
                    seen.push(b);
                    met.push((a.min(b), a.max(b)));
                }
            }
        }
        met.sort_unstable();
        met.dedup();
        let n = usize::from(teams);
        assert_eq!(met.len(), n * (n - 1) / 2, "{teams} teams");
    }
}

#[test]
fn scoring_settles_series_and_carries_winners_up_the_bracket() {
    let mut cup = info(TournamentFormat::SingleElimination, 4, 3);
    assert_eq!(current_round(&cup), Some(0));
    assert_eq!(playable(&cup), vec![(0, 0), (0, 1)]);
    let game = |winner: u8| GameInfo {
        match_id: None,
        winner: Some(winner),
    };
    assert!(!score(&mut cup, 0, 0, game(3)));
    assert!(!score(&mut cup, 0, 0, game(0)));
    assert_eq!(cup.rounds[0].pairs[0].winner, None, "best of three");
    assert!(!score(&mut cup, 0, 0, game(0)));
    assert_eq!(cup.rounds[0].pairs[0].winner, Some(0));
    assert_eq!(cup.rounds[1].pairs[0].a, Some(0), "carried up");
    assert_eq!(playable(&cup), vec![(0, 1)], "the final waits");
    score(&mut cup, 0, 1, game(2));
    score(&mut cup, 0, 1, game(2));
    assert_eq!(cup.rounds[1].pairs[0].b, Some(2));
    assert_eq!(current_round(&cup), Some(1));
    score(&mut cup, 1, 0, game(2));
    assert!(score(&mut cup, 1, 0, game(2)));
    assert_eq!(cup.stage, TournamentStage::Finished);
    assert_eq!(cup.winner, Some(2));
}

#[test]
fn round_robin_standings_count_series_then_games() {
    let mut robin = info(TournamentFormat::RoundRobin, 3, 1);
    let game = |winner: u8| GameInfo {
        match_id: None,
        winner: Some(winner),
    };
    // Round 0: 0 vs 2 (1 has the bye); round 1: 0 vs 1; round 2: 1 vs 2.
    let pairs: Vec<(usize, usize, u8, u8)> = robin
        .rounds
        .iter()
        .enumerate()
        .flat_map(|(r, round)| {
            round
                .pairs
                .iter()
                .enumerate()
                .filter_map(move |(p, pair)| Some((r, p, pair.a?, pair.b?)))
        })
        .collect();
    assert_eq!(pairs.len(), 3);
    let mut finished = false;
    for (r, p, a, b) in pairs {
        // Team 1 beats everybody; team 0 beats team 2.
        let winner = if a == 1 || b == 1 { 1 } else { a.min(b) };
        finished = score(&mut robin, r, p, game(winner));
    }
    assert!(finished);
    assert_eq!(standings(&robin), vec![1, 0, 2]);
    assert_eq!(robin.winner, Some(1));
}

#[test]
fn a_tournament_seats_its_teams_and_a_no_show_forfeits_the_game() {
    let (addr, _dir) = hub("cup");
    let (mut alpha, _, _) = sign_up(addr, "alpha");
    let (mut beta, _, _) = sign_up(addr, "beta");
    alpha.send(&ToHub::CreateTournament {
        name: "cup".to_string(),
        mode: REALTIME,
        format: TournamentFormat::SingleElimination,
        best_of: 2,
    });
    assert_eq!(alpha.refusal(), HallReason::BadSeries);
    alpha.send(&ToHub::CreateTournament {
        name: "cup".to_string(),
        mode: REALTIME,
        format: TournamentFormat::SingleElimination,
        best_of: 1,
    });
    let cup = alpha.recv_until(|msg| match msg {
        FromHub::TournamentState { state } => Some(state),
        _ => None,
    });
    assert_eq!(cup.stage, TournamentStage::Registration);
    alpha.send(&ToHub::StartTournament { id: cup.id });
    assert_eq!(alpha.refusal(), HallReason::TooFewTeams);
    alpha.send(&ToHub::JoinTournament {
        id: cup.id,
        team: "reds".to_string(),
        seats: 1,
    });
    beta.send(&ToHub::JoinTournament {
        id: cup.id,
        team: "blues".to_string(),
        seats: 1,
    });
    let two = beta.recv_until(|msg| match msg {
        FromHub::TournamentState { state } if state.teams.len() == 2 => Some(state),
        _ => None,
    });
    assert_eq!(
        two.teams[1].members,
        vec![bota_proto::MemberInfo {
            login: "beta".to_string(),
            seats: 1
        }]
    );
    beta.send(&ToHub::StartTournament { id: cup.id });
    assert_eq!(beta.refusal(), HallReason::NotOrganizer);
    // Beta watches a match that has started elsewhere, which keeps it there.
    let (mut gamma, _, _) = sign_up(addr, "gamma");
    let (mut delta, _, _) = sign_up(addr, "delta");
    gamma.send(&ToHub::CreateMatch {
        mode: REALTIME,
        name: "elsewhere".to_string(),
        seed: None,
        seats: 1,
    });
    let elsewhere = gamma.match_state().id;
    delta.send(&ToHub::JoinMatch {
        id: elsewhere,
        role: Role::Player { seats: 1 },
    });
    delta.match_state();
    gamma.send(&ToHub::StartMatch);
    gamma.recv_until(|msg| matches!(msg, FromHub::Play { .. }).then_some(()));
    beta.send(&ToHub::JoinMatch {
        id: elsewhere,
        role: Role::Spectator,
    });
    beta.match_state();
    alpha.send(&ToHub::StartTournament { id: cup.id });

    // Alpha was in the hall, so it is brought into the pairing's match, on
    // a fixed seat; beta's seat waits for it.
    let a_state = alpha.match_state();
    assert_eq!(a_state.host, None);
    assert_eq!(a_state.yours.first().copied(), Some(SlotId(0)));
    assert_eq!(a_state.slots[1].login, "beta");
    assert!(!a_state.slots[1].connected);
    alpha.send(&ToHub::TakeSeat { slot: SlotId(1) });
    assert_eq!(alpha.refusal(), HallReason::SeatsFixed);

    // The deadline passes and blues, who never came, forfeit.
    let finished = alpha.recv_until(|msg| match msg {
        FromHub::TournamentState { state } if state.stage == TournamentStage::Finished => {
            Some(state)
        }
        _ => None,
    });
    assert_eq!(finished.winner, Some(0));
    let game = finished.rounds[0].pairs[0].games[0];
    assert_eq!(game.match_id, None, "a walkover");
    assert_eq!(game.winner, Some(0));
    alpha.hall();
}

#[test]
fn a_cup_pairs_teams_of_the_size_its_mode_asks_for() {
    let (addr, _dir) = hub("teams");
    let mut players: Vec<TestClient> = ["alpha", "beta", "gamma", "delta"]
        .into_iter()
        .map(|login| sign_up(addr, login).0)
        .collect();
    players[0].send(&ToHub::CreateTournament {
        name: "cup".to_string(),
        mode: TWO_A_SIDE,
        format: TournamentFormat::SingleElimination,
        best_of: 1,
    });
    let cup = players[0].recv_until(|msg| match msg {
        FromHub::TournamentState { state } => Some(state),
        _ => None,
    });
    assert_eq!(cup.team_size, 2, "a four-seat mode wants teams of two");

    for (at, team) in [(0, "reds"), (1, "reds"), (2, "blues")] {
        players[at].send(&ToHub::JoinTournament {
            id: cup.id,
            team: team.to_string(),
            seats: 1,
        });
    }
    let three = players[0].recv_until(|msg| match msg {
        FromHub::TournamentState { state }
            if state.teams.len() == 2 && state.teams[0].members.len() == 2 =>
        {
            Some(state)
        }
        _ => None,
    });
    assert_eq!(three.teams[1].members.len(), 1, "blues is a player short");
    players[0].send(&ToHub::StartTournament { id: cup.id });
    assert_eq!(
        players[0].refusal(),
        HallReason::TooFewTeams,
        "a team short of members is not paired"
    );

    players[3].send(&ToHub::JoinTournament {
        id: cup.id,
        team: "blues".to_string(),
        seats: 1,
    });
    players[3].recv_until(|msg| match msg {
        FromHub::TournamentState { state } if state.teams[1].members.len() == 2 => Some(()),
        _ => None,
    });
    players[0].send(&ToHub::StartTournament { id: cup.id });
    // Every one of the four is seated, two a side, in the one game.
    let mut seats = Vec::new();
    for player in &mut players {
        let state = player.match_state();
        seats.push((
            state.id,
            state.yours.first().copied().expect("a seat"),
            state.slots.len(),
        ));
    }
    assert!(
        seats.iter().all(|s| s.0 == seats[0].0),
        "one game: {seats:?}"
    );
    assert!(seats.iter().all(|s| s.2 == 4), "four seats: {seats:?}");
    let mut held: Vec<u8> = seats.iter().map(|s| s.1.0).collect();
    held.sort_unstable();
    assert_eq!(held, vec![0, 1, 2, 3], "one seat each");
}

#[test]
fn a_member_holds_as_many_of_its_teams_seats_as_its_mode_allows() {
    let (addr, _dir) = hub("team-seats");
    let mut players: Vec<TestClient> = ["alpha", "beta", "gamma"]
        .into_iter()
        .map(|login| sign_up(addr, login).0)
        .collect();
    fn cup_in(organizer: &mut TestClient, mode: bota_proto::ModeId) -> bota_proto::TournamentId {
        organizer.send(&ToHub::CreateTournament {
            name: "cup".to_string(),
            mode,
            format: TournamentFormat::SingleElimination,
            best_of: 1,
        });
        organizer.recv_until(|msg| match msg {
            FromHub::TournamentState { state } if state.mode == mode => Some(state.id),
            _ => None,
        })
    }
    // A cup of one seat to an account seats nobody on two.
    let solo = cup_in(&mut players[0], TWO_A_SIDE);
    players[0].send(&ToHub::JoinTournament {
        id: solo,
        team: "reds".to_string(),
        seats: 2,
    });
    assert_eq!(players[0].refusal(), HallReason::TooManySeats);
    players[0].send(&ToHub::CancelTournament { id: solo });

    // In an arena alpha is a whole team on its own.
    let id = cup_in(&mut players[0], CREW_LOCKSTEP);
    players[0].send(&ToHub::JoinTournament {
        id,
        team: "reds".to_string(),
        seats: 2,
    });
    players[1].send(&ToHub::JoinTournament {
        id,
        team: "reds".to_string(),
        seats: 1,
    });
    assert_eq!(players[1].refusal(), HallReason::TeamFull, "reds is full");
    for at in [1, 2] {
        players[at].send(&ToHub::JoinTournament {
            id,
            team: "blues".to_string(),
            seats: 1,
        });
    }
    players[2].recv_until(|msg| match msg {
        FromHub::TournamentState { state }
            if state.teams.len() == 2 && state.teams[1].members.len() == 2 =>
        {
            Some(())
        }
        _ => None,
    });
    players[0].send(&ToHub::StartTournament { id });
    assert_eq!(
        players[0].match_state().yours.len(),
        2,
        "alpha drives both of its team's seats"
    );
    assert_eq!(players[1].match_state().yours.len(), 1);
    assert_eq!(players[2].match_state().yours.len(), 1);
}

#[test]
fn a_cup_game_fetches_its_players_out_of_whatever_lobby_they_were_waiting_in() {
    let (addr, _dir) = hub("fetch-players");
    let (mut alpha, _, _) = sign_up(addr, "alpha");
    let (mut beta, _, _) = sign_up(addr, "beta");
    alpha.send(&ToHub::CreateTournament {
        name: "cup".to_string(),
        mode: REALTIME,
        format: TournamentFormat::SingleElimination,
        best_of: 1,
    });
    let cup = alpha.recv_until(|msg| match msg {
        FromHub::TournamentState { state } => Some(state),
        _ => None,
    });
    for (client, team) in [(&mut alpha, "reds"), (&mut beta, "blues")] {
        client.send(&ToHub::JoinTournament {
            id: cup.id,
            team: team.to_string(),
            seats: 1,
        });
    }
    beta.recv_until(|msg| match msg {
        FromHub::TournamentState { state } if state.teams.len() == 2 => Some(()),
        _ => None,
    });
    // Beta goes off to wait in a lobby of its own while the cup starts.
    beta.send(&ToHub::CreateMatch {
        mode: REALTIME,
        name: "somewhere else".to_string(),
        seed: None,
        seats: 1,
    });
    let elsewhere = beta.match_state();
    alpha.send(&ToHub::StartTournament { id: cup.id });

    let seated = beta.recv_until(|msg| match msg {
        FromHub::MatchState { state } if state.id != elsewhere.id => Some(state),
        _ => None,
    });
    assert_eq!(
        seated.yours.first().copied(),
        Some(SlotId(1)),
        "beta is brought to its game"
    );
    assert_eq!(seated.host, None, "and it is the cup's game");
    // Each arrival pushes a state, so the one that matters is the state
    // where both are in.
    let with_alpha = alpha.recv_until(|msg| match msg {
        FromHub::MatchState { state } if state.slots[1].connected => Some(state),
        _ => None,
    });
    assert_eq!(with_alpha.id, seated.id, "both in the one game");
    assert_eq!(with_alpha.slots[1].login, "beta");
    assert_eq!(with_alpha.yours.first().copied(), Some(SlotId(0)));
}

#[test]
fn a_cup_is_taken_down_by_its_organiser_and_by_nobody_else() {
    let (addr, _dir) = hub("cancel");
    let (mut alpha, _, _) = sign_up(addr, "alpha");
    let (mut beta, _, _) = sign_up(addr, "beta");
    alpha.send(&ToHub::CreateTournament {
        name: "cup".to_string(),
        mode: REALTIME,
        format: TournamentFormat::RoundRobin,
        best_of: 3,
    });
    let cup = alpha.recv_until(|msg| match msg {
        FromHub::TournamentState { state } => Some(state),
        _ => None,
    });
    assert_eq!(cup.best_of, 3, "the series length is the one asked for");
    assert_eq!(cup.format, TournamentFormat::RoundRobin);
    beta.send(&ToHub::JoinTournament {
        id: cup.id,
        team: "blues".to_string(),
        seats: 1,
    });
    beta.recv_until(|msg| match msg {
        FromHub::TournamentState { .. } => Some(()),
        _ => None,
    });
    beta.send(&ToHub::CancelTournament { id: cup.id });
    assert_eq!(beta.refusal(), HallReason::NotOrganizer);

    alpha.send(&ToHub::CancelTournament { id: cup.id });
    assert_eq!(
        beta.refusal(),
        HallReason::NoSuchTournament,
        "whoever was in it is told it is gone"
    );
    // Hall pushes queue up, so the one that matters is the first without it.
    alpha.recv_until(|msg| match msg {
        FromHub::Hall { tournaments, .. } if tournaments.iter().all(|t| t.id != cup.id) => Some(()),
        _ => None,
    });
    alpha.send(&ToHub::StartTournament { id: cup.id });
    assert_eq!(alpha.refusal(), HallReason::NoSuchTournament);
}

#[test]
fn an_abandoned_tournament_game_is_played_again() {
    let (addr, _dir) = hub("replayed");
    let (mut alpha, _, _) = sign_up(addr, "alpha");
    let (mut beta, _, _) = sign_up(addr, "beta");
    alpha.send(&ToHub::CreateTournament {
        name: "cup".to_string(),
        mode: REALTIME,
        format: TournamentFormat::RoundRobin,
        best_of: 1,
    });
    let cup = alpha.recv_until(|msg| match msg {
        FromHub::TournamentState { state } => Some(state),
        _ => None,
    });
    alpha.send(&ToHub::JoinTournament {
        id: cup.id,
        team: "reds".to_string(),
        seats: 1,
    });
    beta.send(&ToHub::JoinTournament {
        id: cup.id,
        team: "blues".to_string(),
        seats: 1,
    });
    beta.recv_until(|msg| match msg {
        FromHub::TournamentState { state } if state.teams.len() == 2 => Some(()),
        _ => None,
    });
    alpha.send(&ToHub::StartTournament { id: cup.id });
    // Both are in, so the game starts at once.
    let first = alpha.match_state();
    beta.match_state();
    alpha.pick(HeroId(0));
    beta.pick(HeroId(0));
    alpha.match_start();
    beta.match_start();
    alpha.snapshot();
    // Both walk out of the game and back to the hall; the game is abandoned,
    // voided, and set up again.
    alpha.leave_game();
    beta.leave_game();
    alpha.send(&ToHub::LeaveMatch);
    beta.send(&ToHub::LeaveMatch);
    alpha.hall();
    beta.hall();
    let again = alpha.recv_until(|msg| match msg {
        FromHub::MatchState { state } if state.id != first.id => Some(state),
        _ => None,
    });
    assert_eq!(again.stage, Stage::Gathering);
    assert_eq!(again.yours.first().copied(), Some(SlotId(0)));
    let running = beta.recv_until(|msg| match msg {
        FromHub::TournamentState { state }
            if state.rounds[0].pairs[0].games.len() == 1
                && state.rounds[0].pairs[0].games[0].match_id == Some(again.id) =>
        {
            Some(state)
        }
        _ => None,
    });
    assert_eq!(running.stage, TournamentStage::Running);
    // A spectator may still watch the new game.
    let mut eye = sign_up(addr, "eye").0;
    eye.send(&ToHub::JoinMatch {
        id: again.id,
        role: Role::Spectator,
    });
    assert_eq!(eye.match_state().yours.first().copied(), None);
}
