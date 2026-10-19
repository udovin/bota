use std::thread;
use std::time::Duration;

use bota_proto::{
    Auth, FrameReader, FromGame, FromHub, HallReason, HeroId, Order, PickReason, RejectReason,
    ReplayRecord, Role, SlotId, Stage, Target, Team, ToGame, ToHub, Vec2,
};

use super::fixtures::*;

#[test]
fn a_realtime_match_reaches_two_clients_and_is_archived_once_abandoned() {
    let (addr, dir) = hub("realtime");
    let (mut c1, mut c2, id) = started(addr, REALTIME);

    let first = c1.snapshot();
    assert_eq!(first.viewer, Some(Team::Radiant));
    assert_eq!(first.players.len(), 2);
    assert!(first.units.iter().any(|u| u.team == Team::Radiant));
    let first_events = c1.game_recv_until(|msg| match msg {
        FromGame::Events { tick, events } if tick == first.tick => Some(events),
        _ => None,
    });
    assert!(first_events.is_empty());

    // A legal order passes silently; an impossible one is named and refused.
    let hero = hero_of(&first, SlotId(0));
    let given = c1.order(&[
        (
            hero,
            Order::Move {
                target: Target::Pos(Vec2::from_ints(1000, 1000)),
            },
        ),
        (
            hero,
            Order::Buy {
                item: bota_proto::ItemId(999),
            },
        ),
    ]);
    assert_eq!(c1.order_refused(), (given[1], RejectReason::UnknownItem));
    let later = c2.snapshot();
    assert_eq!(later.viewer, Some(Team::Dire));

    // Hanging up empties the seats; nobody comes back, so the match is
    // abandoned, archived and its replay closed.
    drop(c1);
    drop(c2);
    let mut eye = sign_up(addr, "eye").0;
    let replay_path = dir.join("replays").join(format!("{}.brp", id.0));
    for _ in 0..100 {
        thread::sleep(Duration::from_millis(50));
        let bytes = std::fs::read(&replay_path).unwrap_or_default();
        if !bytes.is_empty() {
            let mut reader = FrameReader::new();
            reader.push(&bytes);
            if let Ok(Some(ReplayRecord::Msg(FromGame::MatchStart { .. }))) =
                reader.next_message::<ReplayRecord>()
            {
                eye.send(&ToHub::ListMatches {
                    filter: bota_proto::MatchFilter::default(),
                    before: None,
                    limit: 10,
                });
                let page = eye.recv_until(|msg| match msg {
                    FromHub::Matches { page, .. } => Some(page),
                    _ => None,
                });
                let ours = page.iter().find(|s| s.id == id).expect("listed");
                assert_eq!(ours.stage, Stage::Abandoned);
                assert_eq!(ours.seed, Some(7));
                assert_eq!(ours.seats[1].login, "beta");
                return;
            }
        }
    }
    panic!("the replay never became readable");
}

#[test]
fn a_seat_holder_who_drops_comes_back_to_the_same_seat() {
    let (addr, _dir) = hub("rejoin");
    let (mut alpha, mut beta, id) = started(addr, REALTIME);
    alpha.snapshot();
    let (_, beta_token) = {
        let mut probe = TestClient::connect(addr);
        probe.send(&ToHub::Hello {
            auth: Auth::Login {
                login: "beta".to_string(),
                password: "beta-pw".to_string(),
            },
        });
        probe.recv_until(|msg| match msg {
            FromHub::Welcome { account, token, .. } => Some((account, token)),
            _ => None,
        })
    };
    // Signing in elsewhere threw beta's first connection out of the hub.
    assert!(beta.hung_up());
    let away = alpha.recv_until(|msg| match msg {
        FromHub::MatchState { state } if !state.slots[1].connected => Some(state),
        _ => None,
    });
    assert_eq!(away.slots[1].login, "beta", "the seat stays beta's");
    // Signing in again leaves beta in the hall, which lists the match with
    // beta's seat in it.
    let mut back = TestClient::connect(addr);
    back.send(&ToHub::Hello {
        auth: Auth::Resume {
            token: beta_token.expect("a token"),
        },
    });
    let listed = back.hall();
    let held = listed
        .iter()
        .find(|m| m.id == id)
        .expect("the match is listed");
    assert_eq!(held.stage, Stage::Playing);
    assert!(
        held.seats
            .iter()
            .any(|seat| seat.login == "beta" && !seat.abandoned),
        "with beta's seat in it"
    );
    // Asking for it puts beta back on its seat and sends it to the same game
    // server with the same ticket, which lets its old connection go.
    back.send(&ToHub::JoinMatch {
        id,
        role: Role::Player { seats: 1 },
    });
    let state = back.match_state();
    assert_eq!(state.id, id);
    assert_eq!(state.yours.first().copied(), Some(SlotId(1)));
    assert_eq!(state.stage, Stage::Playing);
    let ticket = back.recv_until(|msg| match msg {
        FromHub::Play { ticket, .. } => Some(ticket),
        _ => None,
    });
    assert_eq!(
        Some(ticket),
        beta.play.as_ref().map(|(_, _, ticket)| *ticket),
        "the same pass"
    );
    back.match_start();
    assert_eq!(back.snapshot().viewer, Some(Team::Dire));
    assert!(beta.game_hung_up(), "one connection a ticket");
    let left = alpha.game_recv_until(|msg| match msg {
        FromGame::ParticipantLeft { slots } => Some(slots),
        _ => None,
    });
    assert_eq!(left, vec![SlotId(1)]);
}

#[test]
fn a_gathering_match_passes_its_host_on_and_closes_when_empty() {
    let (addr, _dir) = hub("host");
    let (mut alpha, mut beta, id) = gathered(addr, REALTIME);
    alpha.send(&ToHub::LeaveMatch);
    let state = beta.match_state();
    assert_eq!(state.slots[0].login, "", "the seat was given up");
    assert_ne!(state.host, None);
    let hall = alpha.hall();
    assert!(hall.iter().any(|m| m.id == id), "still gathering");
    // Seats are added, not swapped: changing sides is giving one up and
    // taking the other.
    beta.send(&ToHub::TakeSeat { slot: SlotId(0) });
    assert_eq!(beta.refusal(), HallReason::SeatsAcrossTeams);
    beta.send(&ToHub::DropSeat { slot: None });
    beta.send(&ToHub::TakeSeat { slot: SlotId(0) });
    let moved = beta.recv_until(|msg| match msg {
        FromHub::MatchState { state } if !state.yours.is_empty() => Some(state),
        _ => None,
    });
    assert_eq!(moved.yours, vec![SlotId(0)]);
    beta.send(&ToHub::LeaveMatch);
    let hall = beta.hall();
    assert!(
        hall.iter().all(|m| m.id != id),
        "a gathering match nobody holds is gone"
    );
}

#[test]
fn a_spectator_borrows_eyes_and_is_told_the_orders_they_see() {
    let (addr, _dir) = hub("eyes");
    let (mut c1, mut c2, id) = started(addr, REALTIME);
    let mut watcher = TestClient::connect(addr);
    watcher.send(&ToHub::Hello {
        auth: Auth::Guest {
            name: "eye".to_string(),
        },
    });
    watcher.send(&ToHub::JoinMatch {
        id,
        role: Role::Spectator,
    });
    let state = watcher.match_state();
    assert_eq!(
        state.yours.first().copied(),
        None,
        "a spectator takes no seat"
    );
    assert_eq!(state.watchers, vec!["eye".to_string()]);
    watcher.match_start();
    let first = watcher.snapshot();
    assert_eq!(
        first.viewer, None,
        "everything, before any eyes are asked for"
    );

    c1.order(&[(
        hero_of(&first, SlotId(0)),
        Order::Move {
            target: Target::Pos(Vec2::from_ints(1000, 1000)),
        },
    )]);
    let mut told_freely = 0u32;
    for _ in 0..3 {
        watcher.game_recv_until(|msg| match msg {
            FromGame::Orders { .. } => {
                told_freely += 1;
                None
            }
            FromGame::Snapshot { .. } => Some(()),
            _ => None,
        });
    }
    assert_eq!(told_freely, 0, "watching everything is told no orders");

    watcher.send_game(&ToGame::ViewAs {
        seat: Some(SlotId(1)),
    });
    let fogged = watcher.game_recv_until(|msg| match msg {
        FromGame::Snapshot { view } if view.viewer.is_some() => Some(view),
        _ => None,
    });
    assert_eq!(fogged.viewer, Some(Team::Dire));

    let dire_hero = hero_of(&fogged, SlotId(1));
    c2.order(&[(
        dire_hero,
        Order::Move {
            target: Target::Pos(Vec2::from_ints(15000, 15000)),
        },
    )]);
    let told = watcher.game_recv_until(|msg| match msg {
        FromGame::Orders { orders, .. } => Some(orders),
        _ => None,
    });
    assert!(
        told.iter().all(|given| given.unit == dire_hero),
        "only the orders of the seat watched through: {told:?}"
    );
    // A guest may watch but never sit.
    watcher.send(&ToHub::JoinMatch {
        id,
        role: Role::Player { seats: 1 },
    });
    assert_eq!(watcher.refusal(), HallReason::GuestsCannotPlay);
}

#[test]
fn lockstep_advances_as_fast_as_the_acks_come() {
    let (addr, _dir) = hub("lockstep");
    let (mut c1, mut c2, _) = started(addr, LOCKSTEP);
    let mut last_tick = 0;
    for _ in 0..5 {
        for client in [&mut c1, &mut c2] {
            let tick = client.snapshot().tick;
            client.send_game(&ToGame::Ack { tick });
            last_tick = last_tick.max(tick);
        }
    }
    assert!(last_tick >= 4, "the match advanced on acknowledgements");
}

#[test]
fn a_lockstep_match_everybody_left_stops_advancing_instead_of_racing_ahead() {
    let (addr, _dir) = hub("empty-lockstep");
    let (mut c1, mut c2, id) = started(addr, LOCKSTEP);
    for client in [&mut c1, &mut c2] {
        let tick = client.snapshot().tick;
        client.send_game(&ToGame::Ack { tick });
    }
    // Nobody is left to acknowledge anything, so nothing may be taken for
    // an acknowledgement. The match waits out its abandon time instead of
    // spending it on ticks nobody asked for.
    drop(c1);
    drop(c2);
    let mut eye = sign_up(addr, "eye").0;
    for _ in 0..200 {
        eye.send(&ToHub::ListMatches {
            filter: bota_proto::MatchFilter::default(),
            before: None,
            limit: 10,
        });
        let page = eye.recv_until(|msg| match msg {
            FromHub::Matches { page, .. } => Some(page),
            _ => None,
        });
        let Some(ours) = page.iter().find(|s| s.id == id) else {
            continue;
        };
        if ours.stage == Stage::Abandoned {
            assert!(
                ours.duration < 100,
                "an empty lockstep match ran {} ticks unasked",
                ours.duration
            );
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("the match was never abandoned");
}

#[test]
fn only_the_host_starts_a_match_and_only_with_every_seat_held_by_somebody_in_it() {
    let (addr, _dir) = hub("start");
    let (mut alpha, _, _) = sign_up(addr, "alpha");
    alpha.send(&ToHub::CreateMatch {
        mode: REALTIME,
        name: "test".to_string(),
        seed: None,
        seats: 1,
    });
    let id = alpha.match_state().id;
    alpha.send(&ToHub::StartMatch);
    assert_eq!(alpha.refusal(), HallReason::NotAllHere, "a seat is open");
    let (mut beta, _, _) = sign_up(addr, "beta");
    beta.send(&ToHub::JoinMatch {
        id,
        role: Role::Player { seats: 1 },
    });
    beta.match_state();
    beta.send(&ToHub::StartMatch);
    assert_eq!(beta.refusal(), HallReason::NotHost);
    alpha.send(&ToHub::StartMatch);
    let stage = alpha.recv_until(|msg| match msg {
        FromHub::MatchState { state } if state.stage != Stage::Gathering => Some(state.stage),
        _ => None,
    });
    assert_eq!(
        stage,
        Stage::Picking,
        "heroes are picked on the game server"
    );
    alpha.send(&ToHub::StartMatch);
    assert_eq!(alpha.refusal(), HallReason::NotGathering);

    // A hero nobody has is no pick. The time runs out, and the match begins
    // with a hero for every seat all the same.
    alpha.picking();
    alpha.send_game(&ToGame::Pick {
        seat: SlotId(0),
        hero: HeroId(99),
    });
    assert_eq!(alpha.pick_refused().2, PickReason::NoSuchHero);
    let info = alpha.match_start();
    assert_eq!(info.picks.len(), 2);
}

#[test]
fn the_host_may_kick_and_nobody_else_may() {
    let (addr, _dir) = hub("kick");
    let (mut alpha, alpha_account, _) = sign_up(addr, "alpha");
    let (mut beta, beta_account, _) = sign_up(addr, "beta");
    alpha.send(&ToHub::CreateMatch {
        mode: REALTIME,
        name: "test".to_string(),
        seed: None,
        seats: 1,
    });
    let opened = alpha.match_state();
    beta.send(&ToHub::JoinMatch {
        id: opened.id,
        role: Role::Player { seats: 1 },
    });
    assert_eq!(beta.match_state().yours.first().copied(), Some(SlotId(1)));

    beta.send(&ToHub::Kick {
        account: alpha_account.id,
    });
    assert_eq!(
        beta.refusal(),
        HallReason::NotHost,
        "only the host throws anybody out"
    );
    alpha.send(&ToHub::Kick {
        account: beta_account.id,
    });
    beta.hall();
    let state = alpha.recv_until(|msg| match msg {
        FromHub::MatchState { state } if state.slots[1].login.is_empty() => Some(state),
        _ => None,
    });
    assert_eq!(
        state.yours.first().copied(),
        Some(SlotId(0)),
        "the host keeps its own seat"
    );
}
