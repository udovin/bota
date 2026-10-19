//! Command line entry point of the servers.

use std::io;
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;

use clap::{Parser, Subcommand};

use bota_server::host::{HostOpts, TICKET_GRACE, host};
use bota_server::hub::{HubOpts, serve};
use bota_server::keys::{fresh, read_key, read_or_make_key};

#[cfg(test)]
mod cli_tests;

/// bota servers: the hub — accounts, the hall, matches before and after they
/// run, tournaments, the archive — and the game servers that run the
/// matches. With no role named, one of each in this process.
#[derive(Parser, Debug)]
#[command(args_conflicts_with_subcommands = true)]
struct Args {
    /// Which server to run alone.
    #[command(subcommand)]
    role: Option<Role>,
    /// The hub and one game server together.
    #[command(flatten)]
    both: Both,
}

/// One server alone.
#[derive(Subcommand, Debug)]
enum Role {
    /// The hub alone. Game servers connect to it to be handed matches.
    Hub(HubArgs),
    /// A game server alone. It connects to a hub and runs what it is handed.
    Game(GameArgs),
}

/// The hub and one game server in one process.
#[derive(clap::Args, Debug)]
struct Both {
    /// Port players reach the hub on.
    #[arg(long, default_value_t = 4455)]
    port: u16,
    /// Port players reach the game server on.
    #[arg(long, default_value_t = 4456)]
    game_port: u16,
    /// Address players are told to reach the game server at. Absent: this
    /// machine and the game port.
    #[arg(long)]
    public: Option<String>,
    /// Where replays, accounts and tournaments are kept.
    #[arg(long, default_value = "data")]
    data: PathBuf,
    /// Honour cheat commands in every mode: gold, levels, items and refreshes
    /// for the asking.
    #[arg(long)]
    cheats: bool,
}

/// The hub alone.
#[derive(clap::Args, Debug)]
struct HubArgs {
    /// Port players reach the hub on.
    #[arg(long, default_value_t = 4455)]
    port: u16,
    /// Address game servers reach the hub at. Only this machine unless it
    /// is told otherwise.
    #[arg(long, default_value = "127.0.0.1:4457")]
    servers: String,
    /// Where replays, accounts and tournaments are kept.
    #[arg(long, default_value = "data")]
    data: PathBuf,
    /// The file holding the key the hub and its game servers share, made
    /// when missing. Absent: `servers.key` in the data directory.
    #[arg(long)]
    key: Option<PathBuf>,
    /// Honour cheat commands in every mode: gold, levels, items and refreshes
    /// for the asking.
    #[arg(long)]
    cheats: bool,
}

/// A game server alone.
#[derive(clap::Args, Debug)]
struct GameArgs {
    /// Address of the hub's port for game servers.
    #[arg(long, default_value = "127.0.0.1:4457")]
    hub: String,
    /// Port players reach this server on.
    #[arg(long, default_value_t = 4456)]
    port: u16,
    /// Address players are told to reach this server at. Absent: this
    /// machine and the port.
    #[arg(long)]
    public: Option<String>,
    /// The file holding the key the hub and its game servers share.
    #[arg(long)]
    key: PathBuf,
    /// Matches it runs at once.
    #[arg(long, default_value_t = 8)]
    capacity: u16,
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    match args.role {
        Some(Role::Hub(hub)) => run_hub(hub),
        Some(Role::Game(game)) => run_game(game),
        None => run_both(args.both),
    }
}

fn run_hub(args: HubArgs) -> io::Result<()> {
    let key_path = args.key.unwrap_or_else(|| args.data.join("servers.key"));
    let key = read_or_make_key(&key_path)?;
    let listener = TcpListener::bind(("0.0.0.0", args.port))?;
    let servers = TcpListener::bind(&args.servers)?;
    println!("bota hub listening on {}", listener.local_addr()?);
    println!(
        "game servers connect at {} with the key in {}",
        servers.local_addr()?,
        key_path.display()
    );
    let mut opts = HubOpts::standard(args.data, key);
    opts.cheats = args.cheats;
    serve(listener, servers, opts)
}

fn run_game(args: GameArgs) -> io::Result<()> {
    let key = read_key(&args.key)?;
    let players = TcpListener::bind(("0.0.0.0", args.port))?;
    println!("bota game server listening on {}", players.local_addr()?);
    let opts = HostOpts {
        hub: args.hub,
        key,
        public: args
            .public
            .unwrap_or_else(|| format!("127.0.0.1:{}", args.port)),
        capacity: args.capacity,
        ticket_grace: TICKET_GRACE,
    };
    host(players, opts)
}

fn run_both(args: Both) -> io::Result<()> {
    let key = fresh();
    let listener = TcpListener::bind(("0.0.0.0", args.port))?;
    let servers = TcpListener::bind(("127.0.0.1", 0))?;
    let servers_addr = servers.local_addr()?;
    let players = TcpListener::bind(("0.0.0.0", args.game_port))?;
    println!("bota hub listening on {}", listener.local_addr()?);
    println!("bota game server listening on {}", players.local_addr()?);
    let mut opts = HubOpts::standard(args.data, key);
    opts.cheats = args.cheats;
    thread::spawn(move || serve(listener, servers, opts));
    let host_opts = HostOpts {
        hub: servers_addr.to_string(),
        key,
        public: args
            .public
            .unwrap_or_else(|| format!("127.0.0.1:{}", args.game_port)),
        capacity: 64,
        ticket_grace: TICKET_GRACE,
    };
    host(players, host_opts)
}
