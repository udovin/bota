use clap::Parser;

use crate::{Args, Role};

#[test]
fn with_no_role_named_the_hub_and_a_game_server_run_together() {
    let args = Args::try_parse_from(["bota-server"]).expect("default arguments");
    assert!(args.role.is_none());
    assert_eq!(args.both.data.to_string_lossy(), "data");
    assert_eq!((args.both.port, args.both.game_port), (4455, 4456));
    assert!(args.both.public.is_none());
    assert!(!args.both.cheats);
}

#[test]
fn every_flag_of_the_two_together_is_taken() {
    let args = Args::try_parse_from([
        "bota-server",
        "--port",
        "5000",
        "--game-port",
        "5001",
        "--public",
        "10.0.0.2:5001",
        "--data",
        "elsewhere",
        "--cheats",
    ])
    .expect("all flags");
    assert_eq!((args.both.port, args.both.game_port), (5000, 5001));
    assert_eq!(args.both.public.as_deref(), Some("10.0.0.2:5001"));
    assert_eq!(args.both.data.to_string_lossy(), "elsewhere");
    assert!(args.both.cheats);
}

#[test]
fn the_hub_alone_keeps_its_servers_port_on_this_machine() {
    let args = Args::try_parse_from(["bota-server", "hub"]).expect("hub");
    let Some(Role::Hub(hub)) = args.role else {
        panic!("the hub role");
    };
    assert_eq!(hub.servers, "127.0.0.1:4457");
    assert!(hub.key.is_none(), "the key lives in the data directory");
}

#[test]
fn a_game_server_alone_needs_the_key() {
    assert!(
        Args::try_parse_from(["bota-server", "game"]).is_err(),
        "no key, no game server"
    );
    let args = Args::try_parse_from([
        "bota-server",
        "game",
        "--key",
        "servers.key",
        "--hub",
        "10.0.0.1:4457",
        "--capacity",
        "2",
    ])
    .expect("game");
    let Some(Role::Game(game)) = args.role else {
        panic!("the game role");
    };
    assert_eq!(game.hub, "10.0.0.1:4457");
    assert_eq!(game.capacity, 2);
    assert_eq!(game.port, 4456);
}

#[test]
fn the_flags_of_the_two_together_do_not_reach_a_role() {
    assert!(Args::try_parse_from(["bota-server", "--cheats", "game", "--key", "k"]).is_err());
}
