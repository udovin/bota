//! Giving a seat of a running match up for good: who may, what the match is
//! told, and how a side with nobody left ends it.

use bota_proto::{FromGame, FromHub, HallReason, HeroId, MatchId, Role, Team, ToHub};

use super::fixtures::*;

#[test]
fn a_seat_holder_who_abandons_from_the_hall_loses_a_one_on_one() {
    let (addr, _dir) = hub("abandon-duel");
    let (mut alpha, mut beta, id) = started(addr, REALTIME);
    // Beta steps out to the hall; the match goes on with its seat away.
    beta.send(&ToHub::LeaveMatch);
    beta.leave_game();
    beta.hall();
    beta.send(&ToHub::Abandon { id });
    assert_eq!(alpha.game_notice(), "beta abandoned the match");
    let winner = alpha.game_recv_until(|msg| match msg {
        FromGame::MatchOver { winner, .. } => Some(winner),
        _ => None,
    });
    assert_eq!(winner, Team::Radiant, "a side with nobody left loses");

    // The hall shows the seat given up, still under beta's name, and it is
    // nobody's to come back to.
    let listed = beta.recv_until(|msg| match msg {
        FromHub::Hall { matches, .. } => matches
            .into_iter()
            .find(|m| m.id == id && m.seats.iter().any(|seat| seat.abandoned)),
        _ => None,
    });
    assert_eq!(listed.seats[1].login, "beta");
    assert!(!listed.seats[0].abandoned, "alpha's seat stays alpha's");
    beta.send(&ToHub::JoinMatch {
        id,
        role: Role::Player { seats: 1 },
    });
    assert_eq!(beta.refusal(), HallReason::NoSeat);
}

#[test]
fn a_side_plays_on_while_a_seat_of_it_is_left_and_loses_with_the_last() {
    let (addr, _dir) = hub("abandon-side");
    let (mut alpha, _, _) = sign_up(addr, "alpha");
    alpha.send(&ToHub::CreateMatch {
        mode: TWO_A_SIDE,
        name: "sides".to_string(),
        seed: Some(7),
        seats: 1,
    });
    let id = alpha.match_state().id;
    // Seats fill in order: beta on the Dire's first, gamma on the Radiant's
    // second, delta on the Dire's second.
    let join = |login: &str| {
        let (mut client, _, _) = sign_up(addr, login);
        client.send(&ToHub::JoinMatch {
            id,
            role: Role::Player { seats: 1 },
        });
        client.recv_until(|msg| match msg {
            FromHub::MatchState { state } if !state.yours.is_empty() => Some(()),
            _ => None,
        });
        client
    };
    let mut beta = join("beta");
    let mut gamma = join("gamma");
    let mut delta = join("delta");
    alpha.send(&ToHub::StartMatch);
    for client in [&mut alpha, &mut beta, &mut gamma, &mut delta] {
        client.pick(HeroId(0));
    }
    for client in [&mut alpha, &mut beta, &mut gamma, &mut delta] {
        client.match_start();
    }

    // Gamma gives its Radiant seat up from inside the match: it is let go
    // of the game server, and the match goes on for alpha.
    gamma.send(&ToHub::Abandon { id });
    assert_eq!(beta.game_notice(), "gamma abandoned the match");
    assert!(
        gamma.game_hung_up(),
        "an abandoned seat is driven by nobody"
    );
    gamma.hall();
    let before = beta.snapshot().tick;
    assert!(
        beta.snapshot().tick > before,
        "a side with a seat left plays on"
    );

    // Alpha was the last of the Radiant.
    alpha.send(&ToHub::Abandon { id });
    let winner = delta.game_recv_until(|msg| match msg {
        FromGame::MatchOver { winner, .. } => Some(winner),
        _ => None,
    });
    assert_eq!(winner, Team::Dire);
}

#[test]
fn only_a_seat_holder_of_a_match_that_has_started_abandons_it() {
    let (addr, _dir) = hub("abandon-refused");
    let (mut alpha, _beta, id) = gathered(addr, REALTIME);
    alpha.send(&ToHub::Abandon { id });
    assert_eq!(
        alpha.refusal(),
        HallReason::NotPlaying,
        "a gathering match is left, not abandoned"
    );
    alpha.send(&ToHub::StartMatch);
    alpha.picking();

    let (mut gamma, _, _) = sign_up(addr, "gamma");
    gamma.send(&ToHub::Abandon { id });
    assert_eq!(gamma.refusal(), HallReason::NoSeat);
    gamma.send(&ToHub::Abandon { id: MatchId(999) });
    assert_eq!(gamma.refusal(), HallReason::NoSuchMatch);
}

#[test]
fn a_side_given_up_while_heroes_are_picked_loses_as_the_match_begins() {
    let (addr, _dir) = hub("abandon-picking");
    let (mut alpha, mut beta, id) = gathered(addr, REALTIME);
    alpha.send(&ToHub::StartMatch);
    alpha.picking();
    beta.picking();
    beta.send(&ToHub::Abandon { id });
    assert_eq!(alpha.game_notice(), "beta abandoned the match");
    // Nobody waits for the picks left: every seat is given a hero, the
    // match begins, and the side with nobody left loses it.
    let info = alpha.match_start();
    assert_eq!(info.picks.len(), 2, "the seat given up has a hero too");
    let winner = alpha.game_recv_until(|msg| match msg {
        FromGame::MatchOver { winner, .. } => Some(winner),
        _ => None,
    });
    assert_eq!(winner, Team::Radiant);
}
