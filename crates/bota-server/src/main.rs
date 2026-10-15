//! Command line entry point of the server.

use std::net::TcpListener;
use std::path::PathBuf;

use clap::Parser;

use bota_server::hub::{HubOpts, serve};

#[cfg(test)]
mod cli_tests;

/// bota server: accounts, the hall, matches and tournaments, for as long
/// as it runs.
#[derive(Parser, Debug)]
struct Args {
    /// Port to listen on.
    #[arg(long, default_value_t = 4455)]
    port: u16,
    /// Where replays, accounts and tournaments are kept.
    #[arg(long, default_value = "data")]
    data: PathBuf,
    /// Honour cheat orders in every mode: gold, levels, items and refreshes
    /// for the asking.
    #[arg(long)]
    cheats: bool,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let listener = TcpListener::bind(("0.0.0.0", args.port))?;
    println!("bota-server listening on {}", listener.local_addr()?);
    let mut opts = HubOpts::standard(args.data);
    opts.cheats = args.cheats;
    serve(listener, opts)
}
