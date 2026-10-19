//! The hub and its game servers: who proves the key, where a match plays,
//! who is let in there, and what becomes of a match whose server falls.

use std::net::TcpListener;
use std::thread;

use bota_proto::{
    Assign, FromGame, FromHub, GameReason, HeroId, Nonce, Pick, Report, SlotId, Stage, Team,
    Ticket, ToGame, ToHub,
};

use super::fixtures::*;
use crate::host::{HostOpts, host};
use crate::keys::{Prover, fresh, prove, proves};

#[test]
fn a_match_waits_for_a_game_server_and_starts_once_one_offers_itself() {
    let opts = test_opts(fresh_dir("waits"));
    let key = opts.key;
    let ports = start_hub_alone(opts);
    let (mut host, mut guest, _) = gathered(ports.hub, REALTIME);
    host.send(&ToHub::StartMatch);
    assert_eq!(host.notice(), "the match waits for a game server with room");
    start_game_server(ports.servers, key);
    host.pick(HeroId(0));
    guest.pick(HeroId(0));
    host.match_start();
    guest.match_start();
    assert_eq!(host.snapshot().viewer, Some(Team::Radiant));
}

#[test]
fn a_game_server_without_the_key_is_let_go_and_handed_nothing() {
    let opts = test_opts(fresh_dir("impostor"));
    let ports = start_hub_alone(opts);
    let players = TcpListener::bind("127.0.0.1:0").expect("bind");
    let impostor = HostOpts {
        hub: ports.servers.to_string(),
        key: fresh(),
        public: players.local_addr().expect("addr").to_string(),
        capacity: 4,
        ticket_grace: TEST_TICKET_GRACE,
    };
    let ended = thread::spawn(move || host(players, impostor))
        .join()
        .expect("the game server's thread");
    assert!(ended.is_err(), "the hub hung up on it");

    let (mut host, _guest, _) = gathered(ports.hub, REALTIME);
    host.send(&ToHub::StartMatch);
    assert_eq!(
        host.notice(),
        "the match waits for a game server with room",
        "no match went to it"
    );
}

#[test]
fn a_game_server_lets_in_no_ticket_it_was_not_given() {
    let opts = test_opts(fresh_dir("tickets"));
    let key = opts.key;
    let ports = start_hub_alone(opts);
    let game = start_game_server(ports.servers, key).to_string();

    let mut stranger = Line::open(&game);
    stranger.send(&ToGame::Join {
        ticket: Ticket(fresh()),
    });
    assert!(matches!(
        stranger.heard::<FromGame>(),
        Heard::Msg(FromGame::Refused {
            reason: GameReason::UnknownTicket
        })
    ));
    assert!(matches!(stranger.heard::<FromGame>(), Heard::Closed));

    let mut rude = Line::open(&game);
    rude.send(&ToGame::Ack { tick: 1 });
    assert!(matches!(
        rude.heard::<FromGame>(),
        Heard::Msg(FromGame::Refused {
            reason: GameReason::NotJoined
        })
    ));
}

#[test]
fn a_player_who_loses_the_game_comes_back_with_its_ticket() {
    let (addr, _dir) = hub("comeback");
    let (mut host, _guest, _) = started(addr, REALTIME);
    host.snapshot();
    host.leave_game();
    host.enter_game();
    host.match_start();
    assert_eq!(host.snapshot().viewer, Some(Team::Radiant));
}

#[test]
fn the_key_is_proved_both_ways_and_a_match_whose_server_falls_is_abandoned() {
    let opts = test_opts(fresh_dir("falls"));
    let key = opts.key;
    let ports = start_hub_alone(opts);
    let mut server = Line::open(&ports.servers.to_string());
    let Heard::Msg(Assign::Challenge { nonce }) = server.heard::<Assign>() else {
        panic!("the hub asks first");
    };
    let mine = Nonce(fresh());
    server.send(&Report::Register {
        proof: prove(&key, Prover::Game, &nonce),
        nonce: mine,
        addr: "127.0.0.1:9".to_string(),
        capacity: 1,
    });
    let Heard::Msg(Assign::Accepted { proof }) = server.heard::<Assign>() else {
        panic!("the proof holds");
    };
    assert!(
        proves(&key, Prover::Hub, &mine, &proof),
        "the hub proves the key too"
    );

    let (mut host, _guest, id) = gathered(ports.hub, REALTIME);
    host.send(&ToHub::StartMatch);
    let Heard::Msg(Assign::Start { setup }) = server.heard::<Assign>() else {
        panic!("the match is handed over");
    };
    assert_eq!(setup.match_id, id);
    assert_eq!(setup.seed, 7);
    assert_eq!(setup.passes.len(), 2, "one pass a player");
    let seats: Vec<(SlotId, Team, &str)> = setup
        .seats
        .iter()
        .map(|seat| (seat.slot, seat.team, seat.login.as_str()))
        .collect();
    assert_eq!(
        seats,
        vec![
            (SlotId(0), Team::Radiant, "alpha"),
            (SlotId(1), Team::Dire, "beta")
        ],
        "every seat and who holds it, the heroes left to pick there"
    );
    assert_eq!(setup.pick_ms, 1500, "with the hub's time to pick");
    // The match has started, and then where it plays is said.
    let picking = host.recv_until(|msg| match msg {
        FromHub::MatchState { state } if state.stage != Stage::Gathering => Some(state.stage),
        FromHub::Play { .. } => panic!("told where to play before the match started"),
        _ => None,
    });
    assert_eq!(picking, Stage::Picking);
    let (addr, ticket) = host.recv_until(|msg| match msg {
        FromHub::Play { addr, ticket, .. } => Some((addr, ticket)),
        _ => None,
    });
    assert_eq!(addr, "127.0.0.1:9", "sent where the server said it is");
    assert!(
        setup.passes.iter().any(|pass| pass.ticket == ticket),
        "with a ticket the server knows"
    );

    server.send(&Report::Picked {
        match_id: id,
        picks: vec![
            Pick {
                slot: SlotId(0),
                team: Team::Radiant,
                hero: HeroId(2),
            },
            Pick {
                slot: SlotId(1),
                team: Team::Dire,
                hero: HeroId(1),
            },
        ],
    });
    let played = host.recv_until(|msg| match msg {
        FromHub::MatchState { state } => Some(state.stage),
        _ => None,
    });
    assert_eq!(played, Stage::Playing, "picked, and played");
    let (mut eye, _, _) = sign_up(ports.hub, "eye");
    let listed = eye.hall();
    let heroes: Vec<Option<HeroId>> = listed
        .iter()
        .find(|m| m.id == id)
        .expect("listed")
        .seats
        .iter()
        .map(|seat| seat.hero)
        .collect();
    assert_eq!(heroes, vec![Some(HeroId(2)), Some(HeroId(1))]);

    server.send(&Report::Replay {
        match_id: id,
        bytes: vec![1, 2, 3],
    });
    drop(server);
    let ended = host.recv_until(|msg| match msg {
        FromHub::MatchState { state } if state.stage != Stage::Playing => Some(state.stage),
        _ => None,
    });
    assert_eq!(ended, Stage::Abandoned, "a match without its server ends");
    host.send(&ToHub::FetchReplay { match_id: id });
    let bytes = host.recv_until(|msg| match msg {
        FromHub::ReplayChunk { bytes, .. } => Some(bytes),
        _ => None,
    });
    assert_eq!(bytes, vec![1, 2, 3], "what came of its replay is kept");
}
