use std::thread;
use std::time::Duration;

use bota_proto::{
    Auth, ClientMsg, FrameReader, HallReason, Order, RejectReason, ReplayRecord, Role, ServerMsg,
    SlotId, Stage, Target, Team, Vec2,
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
    let first_events = c1.recv_until(|msg| match msg {
        ServerMsg::Events { tick, events } if tick == first.tick => Some(events),
        _ => None,
    });
    assert!(first_events.is_empty());

    // A legal order passes silently; an impossible one is named and refused.
    c1.send(&ClientMsg::Order {
        seq: 1,
        unit: None,
        order: Order::Move {
            target: Target::Pos(Vec2::from_ints(1000, 1000)),
        },
    });
    c1.send(&ClientMsg::Order {
        seq: 2,
        unit: None,
        order: Order::Buy {
            item: bota_proto::ItemId(999),
        },
    });
    let (seq, reason) = c1.recv_until(|msg| match msg {
        ServerMsg::OrderRejected { seq, reason } => Some((seq, reason)),
        _ => None,
    });
    assert_eq!(seq, 2);
    assert_eq!(reason, RejectReason::UnknownItem);
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
            if let Ok(Some(ReplayRecord::Msg(ServerMsg::MatchStart { .. }))) =
                reader.next_message::<ReplayRecord>()
            {
                eye.send(&ClientMsg::ListMatches {
                    filter: bota_proto::MatchFilter::default(),
                    before: None,
                    limit: 10,
                });
                let page = eye.recv_until(|msg| match msg {
                    ServerMsg::Matches { page, .. } => Some(page),
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
        probe.send(&ClientMsg::Hello {
            auth: Auth::Login {
                login: "beta".to_string(),
                password: "beta-pw".to_string(),
            },
        });
        probe.recv_until(|msg| match msg {
            ServerMsg::Welcome { account, token, .. } => Some((account, token)),
            _ => None,
        })
    };
    // Signing in elsewhere threw beta's first connection out of the match.
    assert!(beta.hung_up());
    let left = alpha.recv_until(|msg| match msg {
        ServerMsg::ParticipantLeft { slot, .. } => Some(slot),
        ServerMsg::MatchState { state } if !state.slots[1].connected => Some(Some(SlotId(1))),
        _ => None,
    });
    assert_eq!(left, Some(SlotId(1)));
    // The probe was put straight back on beta's seat.
    let mut back = TestClient::connect(addr);
    back.send(&ClientMsg::Hello {
        auth: Auth::Resume {
            token: beta_token.expect("a token"),
        },
    });
    let state = back.match_state();
    assert_eq!(state.id, id);
    assert_eq!(state.you, Some(SlotId(1)));
    assert_eq!(state.stage, Stage::Playing);
    back.match_start();
    assert_eq!(back.snapshot().viewer, Some(Team::Dire));
}

#[test]
fn a_gathering_match_passes_its_host_on_and_closes_when_empty() {
    let (addr, _dir) = hub("host");
    let (mut alpha, mut beta, id) = gathered(addr, REALTIME);
    alpha.send(&ClientMsg::LeaveMatch);
    let state = beta.match_state();
    assert_eq!(state.slots[0].login, "", "the seat was given up");
    assert_ne!(state.host, None);
    let hall = alpha.hall();
    assert!(hall.iter().any(|m| m.id == id), "still gathering");
    beta.send(&ClientMsg::TakeSeat {
        slot: Some(SlotId(0)),
    });
    let moved = beta.match_state();
    assert_eq!(moved.you, Some(SlotId(0)));
    beta.send(&ClientMsg::LeaveMatch);
    let hall = beta.hall();
    assert!(
        hall.iter().all(|m| m.id != id),
        "a gathering match nobody holds is gone"
    );
}

#[test]
fn quick_play_fills_an_open_seat_before_opening_another_match() {
    let (addr, _dir) = hub("quick");
    let (mut alpha, _, _) = sign_up(addr, "alpha");
    let (mut beta, _, _) = sign_up(addr, "beta");
    let (mut gamma, _, _) = sign_up(addr, "gamma");
    alpha.send(&ClientMsg::QuickPlay { mode: REALTIME });
    let first = alpha.match_state();
    beta.send(&ClientMsg::QuickPlay { mode: REALTIME });
    let joined = beta.match_state();
    assert_eq!(joined.id, first.id);
    assert_eq!(joined.you, Some(SlotId(1)));
    gamma.send(&ClientMsg::QuickPlay { mode: REALTIME });
    let another = gamma.match_state();
    assert_ne!(another.id, first.id, "a full match is not joined");
    assert_eq!(another.you, Some(SlotId(0)));
    gamma.send(&ClientMsg::QuickPlay {
        mode: bota_proto::ModeId(99),
    });
    assert_eq!(gamma.refusal(), HallReason::NoSuchMode);
}

#[test]
fn a_spectator_borrows_eyes_and_is_told_the_orders_they_see() {
    let (addr, _dir) = hub("eyes");
    let (mut c1, mut c2, id) = started(addr, REALTIME);
    let mut watcher = TestClient::connect(addr);
    watcher.send(&ClientMsg::Hello {
        auth: Auth::Guest {
            name: "eye".to_string(),
        },
    });
    watcher.send(&ClientMsg::JoinMatch {
        id,
        role: Role::Spectator,
    });
    let state = watcher.match_state();
    assert_eq!(state.you, None, "a spectator takes no seat");
    assert_eq!(state.watchers, vec!["eye".to_string()]);
    watcher.match_start();
    let first = watcher.snapshot();
    assert_eq!(
        first.viewer, None,
        "everything, before any eyes are asked for"
    );

    c1.send(&ClientMsg::Order {
        seq: 1,
        unit: None,
        order: Order::Move {
            target: Target::Pos(Vec2::from_ints(1000, 1000)),
        },
    });
    let mut told_freely = 0u32;
    for _ in 0..3 {
        watcher.recv_until(|msg| match msg {
            ServerMsg::Orders { .. } => {
                told_freely += 1;
                None
            }
            ServerMsg::Snapshot { .. } => Some(()),
            _ => None,
        });
    }
    assert_eq!(told_freely, 0, "watching everything is told no orders");

    watcher.send(&ClientMsg::ViewAs {
        seat: Some(SlotId(1)),
    });
    let fogged = watcher.recv_until(|msg| match msg {
        ServerMsg::Snapshot { view } if view.viewer.is_some() => Some(view),
        _ => None,
    });
    assert_eq!(fogged.viewer, Some(Team::Dire));

    c2.send(&ClientMsg::Order {
        seq: 1,
        unit: None,
        order: Order::Move {
            target: Target::Pos(Vec2::from_ints(15000, 15000)),
        },
    });
    let told = watcher.recv_until(|msg| match msg {
        ServerMsg::Orders { orders, .. } => Some(orders),
        _ => None,
    });
    assert!(told.iter().all(|given| given.slot == SlotId(1)));
    // A guest may watch but never sit.
    watcher.send(&ClientMsg::JoinMatch {
        id,
        role: Role::Player,
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
            client.send(&ClientMsg::Ack { tick });
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
        client.send(&ClientMsg::Ack { tick });
    }
    // Nobody is left to acknowledge anything, so nothing may be taken for
    // an acknowledgement. The match waits out its abandon time instead of
    // spending it on ticks nobody asked for.
    drop(c1);
    drop(c2);
    let mut eye = sign_up(addr, "eye").0;
    for _ in 0..200 {
        eye.send(&ClientMsg::ListMatches {
            filter: bota_proto::MatchFilter::default(),
            before: None,
            limit: 10,
        });
        let page = eye.recv_until(|msg| match msg {
            ServerMsg::Matches { page, .. } => Some(page),
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
fn the_host_may_kick_and_nobody_else_may() {
    let (addr, _dir) = hub("kick");
    let (mut alpha, alpha_account, _) = sign_up(addr, "alpha");
    let (mut beta, beta_account, _) = sign_up(addr, "beta");
    alpha.send(&ClientMsg::CreateMatch {
        mode: REALTIME,
        name: "test".to_string(),
        seed: None,
    });
    let opened = alpha.match_state();
    beta.send(&ClientMsg::JoinMatch {
        id: opened.id,
        role: Role::Player,
    });
    assert_eq!(beta.match_state().you, Some(SlotId(1)));

    beta.send(&ClientMsg::Kick {
        account: alpha_account.id,
    });
    assert_eq!(
        beta.refusal(),
        HallReason::NotHost,
        "only the host throws anybody out"
    );
    alpha.send(&ClientMsg::Kick {
        account: beta_account.id,
    });
    beta.hall();
    let state = alpha.recv_until(|msg| match msg {
        ServerMsg::MatchState { state } if state.slots[1].login.is_empty() => Some(state),
        _ => None,
    });
    assert_eq!(state.you, Some(SlotId(0)), "the host keeps its own seat");
}
