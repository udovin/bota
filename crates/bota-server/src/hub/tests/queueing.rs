//! The matchmaking queue: waiting for a mode, accepting what is found, and
//! the time to pick a match from it is given.

use bota_proto::{
    AccountId, FromGame, FromHub, HallReason, HeroId, MatchId, ModeId, PickReason, PlayerId, Role,
    Search, SlotId, Stage, Team, ToGame, ToHub,
};

use super::fixtures::*;
use crate::hub::{HubOpts, Searcher, fill_sides};

fn searcher(turn: u64, mode: u16, seats: u8) -> Searcher {
    Searcher {
        player: PlayerId(turn as u32),
        account: AccountId(turn),
        mode: ModeId(mode),
        seats,
        turn,
    }
}

#[test]
fn sides_fill_in_queue_order_and_a_group_with_no_room_waits() {
    let queue = vec![
        searcher(4, 0, 1),
        searcher(1, 0, 1),
        searcher(2, 1, 1),
        searcher(3, 0, 1),
    ];
    assert_eq!(
        fill_sides(&queue, ModeId(0), 1),
        Some(vec![(1, Team::Radiant), (3, Team::Dire)]),
        "the two who came first of the mode, the first on the Radiant"
    );
    assert_eq!(fill_sides(&queue, ModeId(1), 1), None, "one is not a match");

    let crews = vec![
        searcher(1, 2, 1),
        searcher(2, 2, 2),
        searcher(3, 2, 2),
        searcher(4, 2, 1),
    ];
    assert_eq!(
        fill_sides(&crews, ModeId(2), 2),
        Some(vec![
            (0, Team::Radiant),
            (1, Team::Dire),
            (3, Team::Radiant)
        ]),
        "a side of two left for a single waits"
    );
    assert_eq!(
        fill_sides(&[searcher(1, 0, 3)], ModeId(0), 2),
        None,
        "a group larger than a side never fits"
    );
}

/// Two accounts waiting for the realtime test mode, with the match found for
/// them. Alpha came first.
fn found_pair(addr: std::net::SocketAddr) -> (TestClient, TestClient, MatchId) {
    let (mut alpha, _, _) = sign_up(addr, "alpha");
    let (mut beta, _, _) = sign_up(addr, "beta");
    alpha.send(&ToHub::FindMatch {
        mode: REALTIME,
        seats: 1,
    });
    assert_eq!(
        alpha.search(),
        Search::Queued {
            mode: REALTIME,
            waiting: 1
        },
        "nobody else waits yet, and nothing is made"
    );
    beta.send(&ToHub::FindMatch {
        mode: REALTIME,
        seats: 1,
    });
    let id = alpha.found();
    assert_eq!(beta.found(), id, "one match for both");
    (alpha, beta, id)
}

#[test]
fn a_match_is_made_once_the_queue_fills_it_and_everybody_accepts() {
    let (addr, _dir) = hub("queue-made");
    let (mut alpha, mut beta, id) = found_pair(addr);
    alpha.send(&ToHub::Accept { id });
    let told = beta.recv_until(|msg| match msg {
        FromHub::Search {
            search: Search::Found {
                accepted: 1, yours, ..
            },
        } => Some(yours),
        _ => None,
    });
    assert!(!told, "beta has not accepted yet");
    beta.send(&ToHub::Accept { id });

    // The search is over before the match is entered.
    alpha.recv_until(|msg| match msg {
        FromHub::Search {
            search: Search::Idle,
        } => Some(()),
        FromHub::MatchState { .. } => panic!("the match came before the search ended"),
        _ => None,
    });
    let state = alpha.match_state();
    assert_eq!(state.id, id);
    assert_eq!(
        state.stage,
        Stage::Picking,
        "it goes to its game server at once"
    );
    assert_eq!(
        state.yours,
        vec![SlotId(0)],
        "the first to wait takes the Radiant"
    );
    assert_eq!(state.host, None, "nobody hosts it");
    assert_eq!(beta.match_state().yours, vec![SlotId(1)]);
    let picking = alpha.picking();
    assert_eq!(picking.yours, vec![SlotId(0)]);
    assert!(picking.ms_left > 0, "the time to pick runs");

    // The seats are the accounts': leaving the pick keeps them, and nobody
    // else may take them.
    beta.send(&ToHub::LeaveMatch);
    beta.hall();
    let (mut gamma, _, _) = sign_up(addr, "gamma");
    gamma.send(&ToHub::JoinMatch {
        id,
        role: Role::Player { seats: 1 },
    });
    assert_eq!(gamma.refusal(), HallReason::NoSeat);
    beta.send(&ToHub::JoinMatch {
        id,
        role: Role::Player { seats: 1 },
    });
    assert_eq!(beta.match_state().yours, vec![SlotId(1)], "back on it");
}

#[test]
fn a_match_declined_falls_through_and_who_accepted_waits_again_first() {
    let (addr, _dir) = hub("queue-declined");
    let (mut alpha, mut beta, id) = found_pair(addr);
    alpha.send(&ToHub::Accept { id });
    beta.send(&ToHub::Decline { id });
    beta.recv_until(|msg| match msg {
        FromHub::Search {
            search: Search::Idle,
        } => Some(()),
        _ => None,
    });
    let waiting = alpha.recv_until(|msg| match msg {
        FromHub::Search {
            search: Search::Queued { waiting, .. },
        } => Some(waiting),
        _ => None,
    });
    assert_eq!(waiting, 1, "alpha waits again");

    let (mut gamma, _, _) = sign_up(addr, "gamma");
    gamma.send(&ToHub::FindMatch {
        mode: REALTIME,
        seats: 1,
    });
    let again = alpha.found();
    assert_ne!(again, id, "a match of its own");
    assert_eq!(gamma.found(), again);
    alpha.send(&ToHub::Accept { id: again });
    gamma.send(&ToHub::Accept { id: again });
    assert_eq!(
        alpha.match_state().yours,
        vec![SlotId(0)],
        "alpha kept its place ahead of gamma"
    );
}

#[test]
fn whoever_does_not_accept_in_time_leaves_the_queue() {
    let (addr, _dir) = hub("queue-late");
    let (mut alpha, mut beta, id) = found_pair(addr);
    alpha.send(&ToHub::Accept { id });
    let waiting = alpha.recv_until(|msg| match msg {
        FromHub::Search {
            search: Search::Queued { waiting, .. },
        } => Some(waiting),
        _ => None,
    });
    assert_eq!(waiting, 1, "alpha accepted and waits again");
    // Beta never answered, and is out of the queue.
    beta.recv_until(|msg| match msg {
        FromHub::Search {
            search: Search::Idle,
        } => Some(()),
        _ => None,
    });
}

#[test]
fn a_search_is_cancelled_and_the_queue_refuses_who_has_a_match_running() {
    let (addr, _dir) = hub("queue-cancel");
    let (_alpha, mut beta, id) = started(addr, REALTIME);
    beta.send(&ToHub::LeaveMatch);
    beta.hall();
    beta.send(&ToHub::FindMatch {
        mode: REALTIME,
        seats: 1,
    });
    assert_eq!(
        beta.refusal(),
        HallReason::MatchInProgress,
        "match {} is still beta's",
        id.0
    );

    let (mut gamma, _, _) = sign_up(addr, "gamma");
    gamma.send(&ToHub::FindMatch {
        mode: REALTIME,
        seats: 1,
    });
    assert!(matches!(gamma.search(), Search::Queued { waiting: 1, .. }));
    gamma.send(&ToHub::CancelSearch);
    assert_eq!(gamma.search(), Search::Idle);
    let (mut delta, _, _) = sign_up(addr, "delta");
    delta.send(&ToHub::FindMatch {
        mode: REALTIME,
        seats: 1,
    });
    assert!(
        matches!(delta.search(), Search::Queued { waiting: 1, .. }),
        "gamma waits no more"
    );
    gamma.send(&ToHub::FindMatch {
        mode: ModeId(99),
        seats: 1,
    });
    assert_eq!(gamma.refusal(), HallReason::NoSuchMode);
}

#[test]
fn an_account_of_many_seats_waits_with_singles_and_takes_its_side() {
    let (addr, _dir) = hub("queue-crew");
    let (mut crew, _, _) = sign_up(addr, "crew");
    let (mut beta, _, _) = sign_up(addr, "beta");
    let (mut gamma, _, _) = sign_up(addr, "gamma");
    // A mode of one seat to an account takes no more, from anybody.
    crew.send(&ToHub::FindMatch {
        mode: TWO_A_SIDE,
        seats: 2,
    });
    assert_eq!(crew.refusal(), HallReason::TooManySeats);
    crew.send(&ToHub::FindMatch {
        mode: CREW_LOCKSTEP,
        seats: 3,
    });
    assert_eq!(crew.refusal(), HallReason::TooManySeats, "past a side");

    crew.send(&ToHub::FindMatch {
        mode: CREW_LOCKSTEP,
        seats: 2,
    });
    crew.search();
    beta.send(&ToHub::FindMatch {
        mode: CREW_LOCKSTEP,
        seats: 1,
    });
    beta.search();
    gamma.send(&ToHub::FindMatch {
        mode: CREW_LOCKSTEP,
        seats: 1,
    });
    let id = crew.found();
    for client in [&mut crew, &mut beta, &mut gamma] {
        client.send(&ToHub::Accept { id });
    }
    assert_eq!(
        crew.match_state().yours,
        vec![SlotId(0), SlotId(2)],
        "the whole Radiant"
    );
    assert_eq!(beta.match_state().yours, vec![SlotId(1)]);
    assert_eq!(gamma.match_state().yours, vec![SlotId(3)]);
}

#[test]
fn the_match_waits_for_a_seat_still_picking_until_its_time_runs_out() {
    let opts = HubOpts {
        pick_timeout: std::time::Duration::from_secs(3),
        ..test_opts(fresh_dir("queue-wait-pick"))
    };
    let addr = start_hub(opts);
    let (mut alpha, mut beta, id) = found_pair(addr);
    alpha.send(&ToHub::Accept { id });
    beta.send(&ToHub::Accept { id });
    let began = std::time::Instant::now();
    alpha.pick(HeroId(1));
    beta.picking();
    let info = beta.match_start();
    assert!(
        began.elapsed() >= std::time::Duration::from_millis(2500),
        "the match began {:?} after the picking did",
        began.elapsed()
    );
    assert_eq!(info.picks[0].hero, HeroId(1));
}

#[test]
fn a_pick_is_final_and_time_to_pick_runs_out_with_a_hero_for_every_seat() {
    let (addr, _dir) = hub("queue-pick");
    let (mut alpha, mut beta, id) = found_pair(addr);
    alpha.send(&ToHub::Accept { id });
    beta.send(&ToHub::Accept { id });
    let picking = alpha.picking();
    assert_eq!(
        picking
            .seats
            .iter()
            .map(|seat| seat.hero)
            .collect::<Vec<_>>(),
        vec![None, None],
        "nobody has picked"
    );
    assert_eq!(picking.seats[1].login, "beta");
    // Alpha picks, and everybody is told; beta does not, and steps out.
    alpha.send_game(&ToGame::Pick {
        seat: SlotId(0),
        hero: HeroId(2),
    });
    assert_eq!(alpha.picking().seats[0].hero, Some(HeroId(2)));
    alpha.send_game(&ToGame::Pick {
        seat: SlotId(0),
        hero: HeroId(1),
    });
    assert_eq!(
        alpha.pick_refused(),
        (SlotId(0), HeroId(1), PickReason::AlreadyPicked),
        "a pick is final"
    );
    alpha.send_game(&ToGame::Pick {
        seat: SlotId(1),
        hero: HeroId(1),
    });
    assert_eq!(alpha.pick_refused().2, PickReason::NotYourSeat);
    beta.send(&ToHub::LeaveMatch);
    beta.hall();
    let info = alpha.match_start();
    assert_eq!(info.picks.len(), 2, "every seat has a hero");
    assert_eq!(info.picks[0].hero, HeroId(2), "alpha's own");
    // Beta's seat is still beta's to come back to.
    beta.send(&ToHub::JoinMatch {
        id,
        role: Role::Player { seats: 1 },
    });
    let back = beta.recv_until(|msg| match msg {
        FromHub::MatchState { state } if state.stage == Stage::Playing => Some(state),
        _ => None,
    });
    assert_eq!(back.yours, vec![SlotId(1)]);
    let view = beta.game_recv_until(|msg| match msg {
        FromGame::Snapshot { view } => Some(view),
        _ => None,
    });
    assert_eq!(view.viewer, Some(Team::Dire));
}
