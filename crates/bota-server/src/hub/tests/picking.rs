//! Heroes picked on the game server: a selection shown to its side, a pick
//! to everybody, and what the time running out gives.

use bota_proto::{FromHub, HeroId, PickReason, PickState, Role, SlotId, ToGame, ToHub};

use super::fixtures::*;

/// What one seat shows in a picking state: its pick and its selection.
fn seat_of(state: &PickState, slot: SlotId) -> (Option<HeroId>, Option<HeroId>) {
    let seat = state
        .seats
        .iter()
        .find(|seat| seat.slot == slot)
        .expect("the seat");
    (seat.hero, seat.selected)
}

#[test]
fn a_selection_is_shown_to_its_side_and_a_pick_to_everybody() {
    let (addr, _dir) = hub("select-sides");
    let (mut alpha, _, _) = sign_up(addr, "alpha");
    alpha.send(&ToHub::CreateMatch {
        mode: TWO_A_SIDE,
        name: "sides".to_string(),
        seed: Some(7),
        seats: 1,
    });
    let id = alpha.match_state().id;
    // Seats fill in turn: beta on the Dire, gamma beside alpha on the
    // Radiant, delta on the Dire.
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
    let (mut eye, _, _) = sign_up(addr, "eye");
    eye.send(&ToHub::JoinMatch {
        id,
        role: Role::Spectator,
    });
    for client in [&mut alpha, &mut beta, &mut gamma, &mut delta, &mut eye] {
        client.picking();
    }

    alpha.send_game(&ToGame::Select {
        seat: SlotId(0),
        hero: HeroId(2),
    });
    assert_eq!(
        seat_of(&gamma.picking(), SlotId(0)),
        (None, Some(HeroId(2))),
        "an ally sees what alpha is set on"
    );
    assert_eq!(
        seat_of(&eye.picking(), SlotId(0)),
        (None, Some(HeroId(2))),
        "and so does a spectator"
    );
    assert_eq!(
        seat_of(&beta.picking(), SlotId(0)),
        (None, None),
        "the other side does not"
    );
    alpha.picking();
    delta.picking();

    alpha.send_game(&ToGame::Pick {
        seat: SlotId(0),
        hero: HeroId(2),
    });
    assert_eq!(
        seat_of(&beta.picking(), SlotId(0)),
        (Some(HeroId(2)), None),
        "a pick is shown to everybody"
    );
    alpha.send_game(&ToGame::Select {
        seat: SlotId(0),
        hero: HeroId(1),
    });
    assert_eq!(
        alpha.pick_refused(),
        (SlotId(0), HeroId(1), PickReason::AlreadyPicked),
        "nothing is selected once a seat has picked"
    );
}

#[test]
fn a_seat_set_on_a_hero_is_given_it_when_the_time_runs_out() {
    let (addr, _dir) = hub("select-timeout");
    let (mut alpha, mut beta, _) = gathered(addr, REALTIME);
    alpha.send(&ToHub::StartMatch);
    alpha.picking();
    beta.picking();
    // Alpha changes its mind once and never picks; beta picks.
    for hero in [HeroId(0), HeroId(2)] {
        alpha.send_game(&ToGame::Select {
            seat: SlotId(0),
            hero,
        });
    }
    beta.send_game(&ToGame::Pick {
        seat: SlotId(1),
        hero: HeroId(1),
    });
    let info = alpha.match_start();
    let heroes: Vec<HeroId> = info.picks.iter().map(|pick| pick.hero).collect();
    assert_eq!(
        heroes,
        vec![HeroId(2), HeroId(1)],
        "the hero last selected, and the one picked"
    );
}
