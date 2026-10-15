use clap::Parser;

use crate::Args;

#[test]
fn the_data_directory_defaults_to_data_beside_the_server() {
    let args = Args::try_parse_from(["bota-server"]).expect("default arguments");
    assert_eq!(args.data.to_string_lossy(), "data");
    assert_eq!(args.port, 4455);
    assert!(!args.cheats);
}

#[test]
fn every_flag_is_taken() {
    let args = Args::try_parse_from([
        "bota-server",
        "--port",
        "5000",
        "--data",
        "elsewhere",
        "--cheats",
    ])
    .expect("all flags");
    assert_eq!(args.port, 5000);
    assert_eq!(args.data.to_string_lossy(), "elsewhere");
    assert!(args.cheats);
}
