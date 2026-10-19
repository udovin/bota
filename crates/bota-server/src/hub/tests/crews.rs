//! One account driving several seats: taking them within its mode's limit,
//! ordering their units in one message, and moving a lockstep match with
//! one acknowledgement.

use bota_proto::{
    EntityId, FromHub, HallReason, HeroId, ItemId, MAX_ORDERS_PER_MESSAGE, MatchId, Order,
    RejectReason, Role, SlotId, Target, ToGame, ToHub, UnitKind, Vec2, WorldView,
};

use super::fixtures::*;

/// Alpha holds the Radiant side of a four-seat arena match, beta the Dire
/// side: each asked for two seats.
fn two_crews(addr: std::net::SocketAddr) -> (TestClient, TestClient, MatchId) {
    let (mut alpha, _, _) = sign_up(addr, "alpha");
    let (mut beta, _, _) = sign_up(addr, "beta");
    alpha.send(&ToHub::CreateMatch {
        mode: CREW_LOCKSTEP,
        name: "crews".to_string(),
        seed: Some(7),
        seats: 2,
    });
    let opened = alpha.match_state();
    assert_eq!(
        opened.yours,
        vec![SlotId(0), SlotId(2)],
        "the whole Radiant side"
    );
    beta.send(&ToHub::JoinMatch {
        id: opened.id,
        role: Role::Player { seats: 2 },
    });
    let dire = beta.recv_until(|msg| match msg {
        FromHub::MatchState { state } if !state.yours.is_empty() => Some(state.yours),
        _ => None,
    });
    assert_eq!(dire, vec![SlotId(1), SlotId(3)], "the whole Dire side");
    (alpha, beta, opened.id)
}

/// The host starts the match and both crews pick a hero for every seat; the
/// match begins. Returns alpha's view of the tick both have seen.
fn start(alpha: &mut TestClient, beta: &mut TestClient) -> WorldView {
    alpha.send(&ToHub::StartMatch);
    alpha.pick(HeroId(0));
    beta.pick(HeroId(0));
    alpha.match_start();
    beta.match_start();
    let mut first = alpha.snapshot();
    let theirs = beta.snapshot();
    while first.tick < theirs.tick {
        first = alpha.snapshot();
    }
    first
}

/// Plays lockstep ticks on from the view last seen, each crew acknowledging
/// once a tick for all its seats. Returns alpha's last view.
fn play(alpha: &mut TestClient, beta: &mut TestClient, seen: WorldView, ticks: u32) -> WorldView {
    let mut last = seen;
    for _ in 0..ticks {
        alpha.send_game(&ToGame::Ack { tick: last.tick });
        beta.send_game(&ToGame::Ack { tick: last.tick });
        last = alpha.snapshot();
        beta.snapshot();
    }
    last
}

fn courier_of(view: &WorldView, slot: SlotId) -> EntityId {
    view.units
        .iter()
        .find(|u| u.kind == UnitKind::Courier && u.owner == Some(slot))
        .map(|u| u.id)
        .expect("a courier")
}

#[test]
fn an_account_holds_up_to_its_modes_seats_on_one_side() {
    let (addr, _dir) = hub("sides");
    // A mode of one seat to an account, as Dota is, gives nobody two.
    let (mut solo, _, _) = sign_up(addr, "solo");
    solo.send(&ToHub::CreateMatch {
        mode: TWO_A_SIDE,
        name: "solo".to_string(),
        seed: Some(7),
        seats: 2,
    });
    assert_eq!(solo.refusal(), HallReason::TooManySeats);
    solo.send(&ToHub::CreateMatch {
        mode: TWO_A_SIDE,
        name: "solo".to_string(),
        seed: Some(7),
        seats: 1,
    });
    solo.match_state();
    solo.send(&ToHub::TakeSeat { slot: SlotId(2) });
    assert_eq!(solo.refusal(), HallReason::TooManySeats);

    // An arena gives an account a side, and no more.
    let (mut alpha, mut beta, id) = two_crews(addr);
    let (mut gamma, _, _) = sign_up(addr, "gamma");
    gamma.send(&ToHub::JoinMatch {
        id,
        role: Role::Player { seats: 1 },
    });
    assert_eq!(gamma.refusal(), HallReason::MatchFull);
    alpha.send(&ToHub::TakeSeat { slot: SlotId(1) });
    assert_eq!(alpha.refusal(), HallReason::SeatTaken);

    // Beta gives a Dire seat up; alpha may not take it from the other side.
    beta.send(&ToHub::DropSeat {
        slot: Some(SlotId(3)),
    });
    let left = beta.recv_until(|msg| match msg {
        FromHub::MatchState { state } if state.yours == vec![SlotId(1)] => Some(state),
        _ => None,
    });
    assert_eq!(left.id, id);
    alpha.send(&ToHub::TakeSeat { slot: SlotId(3) });
    assert_eq!(alpha.refusal(), HallReason::SeatsAcrossTeams);
    // Beta takes it back, one at a time.
    beta.send(&ToHub::TakeSeat { slot: SlotId(3) });
    let back = beta.recv_until(|msg| match msg {
        FromHub::MatchState { state } if state.yours.len() == 2 => Some(state.yours),
        _ => None,
    });
    assert_eq!(back, vec![SlotId(1), SlotId(3)]);

    // Giving every seat up makes a watcher.
    beta.send(&ToHub::DropSeat { slot: None });
    let watching = beta.recv_until(|msg| match msg {
        FromHub::MatchState { state } if state.yours.is_empty() => Some(state),
        _ => None,
    });
    assert_eq!(watching.watchers, vec!["beta".to_string()]);
}

#[test]
fn one_message_moves_two_heroes_and_a_courier_on_one_tick() {
    let (addr, _dir) = hub("one-message");
    let (mut alpha, mut beta, _) = two_crews(addr);
    let first_view = start(&mut alpha, &mut beta);
    // One acknowledgement a tick from each connection is enough for all its
    // seats: the match advances.
    let before = play(&mut alpha, &mut beta, first_view, 3);
    assert!(before.tick >= 3, "lockstep moved on one ack per crew");

    let first = hero_of(&before, SlotId(0));
    let second = hero_of(&before, SlotId(2));
    let courier = courier_of(&before, SlotId(0));
    let at = |view: &WorldView| {
        [first, second, courier]
            .into_iter()
            .map(|unit| where_is(view, unit))
            .collect::<Vec<Vec2>>()
    };
    let start_at = at(&before);
    // A hero and the courier of the same seat, and a second seat's hero, all
    // in one message: none of them eats another's order.
    let goal = Order::Move {
        target: Target::Pos(Vec2::from_ints(6000, 6000)),
    };
    alpha.order(&[(first, goal), (courier, goal), (second, goal)]);
    let after = play(&mut alpha, &mut beta, before.clone(), 40);
    let end_at = at(&after);
    for (unit, (from, to)) in ["first hero", "courier", "second hero"]
        .into_iter()
        .zip(start_at.into_iter().zip(end_at))
    {
        assert_ne!(from, to, "the {unit} walked");
    }
}

#[test]
fn orders_for_units_not_driven_or_past_the_limit_are_refused_by_number() {
    let (addr, _dir) = hub("refusals");
    let (mut alpha, mut beta, _) = two_crews(addr);
    let first_view = start(&mut alpha, &mut beta);
    let view = play(&mut alpha, &mut beta, first_view, 1);
    let theirs = hero_of(&view, SlotId(1));
    let courier = courier_of(&view, SlotId(2));
    let walk = Order::Move {
        target: Target::Pos(Vec2::from_ints(6000, 6000)),
    };
    // An enemy hero is not ours to order. A purchase named for a courier is
    // the seat's, and goes through: no refusal comes for it.
    let tango = Order::Buy {
        item: ItemId(crate::game::ITEM_TANGO),
    };
    let given = alpha.order(&[(theirs, walk), (courier, tango)]);
    assert_eq!(
        alpha.order_refused(),
        (given[0], RejectReason::NotYourUnit),
        "the refusal names the order it turns down"
    );

    // Past the limit each order is turned away; the courier is told to walk
    // last, so the one refusal names it.
    let mine = hero_of(&view, SlotId(0));
    let mut flood: Vec<(EntityId, Order)> =
        (0..MAX_ORDERS_PER_MESSAGE).map(|_| (mine, walk)).collect();
    flood.push((courier, walk));
    let given = alpha.order(&flood);
    assert_eq!(
        alpha.order_refused(),
        (given[MAX_ORDERS_PER_MESSAGE], RejectReason::TooManyOrders),
        "only the one past the limit is turned away"
    );
}
