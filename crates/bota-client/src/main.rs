//! bota client: rendering, input, the hall, matches and replays.
//!
//! The client draws what the server sends and sends back intents. It holds no
//! `World` and cannot: the simulation lives in a crate this one does not
//! depend on.

mod camera;
mod catalog;
mod console;
mod hall;
mod hud;
mod icons;
mod input;
mod net;
mod render;
mod replay_play;
mod slots;
mod state;

#[cfg(test)]
mod replay_tests;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use bota_proto::{Auth, ClientMsg, MatchId, ModeId, Token};
use clap::Parser;
use macroquad::prelude::{Conf, next_frame};

use crate::net::Net;
use crate::replay_play::ReplayPlayer;
use crate::state::{App, Phase, Source};

/// bota client: rendering, input, the hall, matches and replays.
#[derive(Parser, Clone, Debug, PartialEq, Eq)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Where the server listens.
    #[arg(long, default_value = "127.0.0.1:4455")]
    pub addr: String,
    /// The account to sign in as. Asked for on screen when absent.
    #[arg(long)]
    pub login: Option<String>,
    /// Its password.
    #[arg(long)]
    pub password: Option<String>,
    /// Register the login instead of signing it in.
    #[arg(long, requires = "login")]
    pub register: bool,
    /// Watch without an account. Takes no seat.
    #[arg(long, conflicts_with_all = ["login", "register"])]
    pub guest: Option<String>,
    /// Where the session token is kept between runs.
    #[arg(long)]
    pub session: Option<PathBuf>,
    /// Take a seat in any open match of this mode as soon as signed in.
    #[arg(long, value_name = "MODE", conflicts_with_all = ["match_id", "watch"])]
    pub quick: Option<u16>,
    /// Enter this match as soon as signed in.
    #[arg(long = "match", value_name = "ID", conflicts_with = "watch")]
    pub match_id: Option<u64>,
    /// Watch this match as soon as signed in.
    #[arg(long, value_name = "ID")]
    pub watch: Option<u64>,
    /// Play this file instead of connecting anywhere.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["addr", "login", "guest"])]
    pub replay: Option<PathBuf>,
}

impl Args {
    /// What to do once the server has signed this client in.
    pub fn errand(&self) -> Option<ClientMsg> {
        if let Some(mode) = self.quick {
            return Some(ClientMsg::QuickPlay { mode: ModeId(mode) });
        }
        if let Some(id) = self.match_id {
            return Some(ClientMsg::JoinMatch {
                id: MatchId(id),
                role: bota_proto::Role::Player,
            });
        }
        self.watch.map(|id| ClientMsg::JoinMatch {
            id: MatchId(id),
            role: bota_proto::Role::Spectator,
        })
    }
}

fn conf() -> Conf {
    Conf {
        window_title: "bota".to_string(),
        window_width: 1280,
        window_height: 800,
        ..Default::default()
    }
}

#[macroquad::main(conf)]
async fn main() {
    let args = Args::parse();
    if args.replay.is_some() {
        render::draw_replay_loading(0);
        next_frame().await;
    }
    let mut app = match build_app(&args) {
        Ok(app) => app,
        Err(err) => {
            eprintln!("bota-client: {err}");
            if args.replay.is_some() {
                replay_open_error(&err.to_string()).await;
            }
            return;
        }
    };
    loop {
        let dt = macroquad::prelude::get_frame_time();
        let msgs = app.pump(dt);
        for msg in msgs {
            app.handle(msg);
        }
        input::handle(&mut app);
        app.check_connection();
        app.tick_effects(dt);
        render::draw(&app);
        if app.quit {
            return;
        }
        next_frame().await;
    }
}

async fn replay_open_error(error: &str) {
    loop {
        render::draw_replay_loading(0);
        render::draw_replay_error(error);
        if macroquad::prelude::is_key_pressed(macroquad::prelude::KeyCode::Escape) {
            return;
        }
        next_frame().await;
    }
}

fn build_app(args: &Args) -> std::io::Result<App> {
    if let Some(path) = &args.replay {
        let player = ReplayPlayer::load(path)?;
        return Ok(App::new(Source::Replay(Box::new(player))));
    }
    let net = Net::connect(&args.addr)?;
    let mut app = App::new(Source::Live(net));
    let session = args.session.clone().unwrap_or_else(session_path);
    let kept = read_session(&session);
    app.session_path = Some(session);
    app.errand = args.errand();
    if let Some(name) = &args.guest {
        app.login = name.clone();
        app.say_hello(Auth::Guest { name: name.clone() });
    } else if let (Some(login), Some(password)) = (&args.login, &args.password) {
        app.login = login.clone();
        app.password = password.clone();
        let auth = if args.register {
            Auth::Register {
                login: login.clone(),
                password: password.clone(),
            }
        } else {
            Auth::Login {
                login: login.clone(),
                password: password.clone(),
            }
        };
        app.say_hello(auth);
    } else if let Some((login, token)) = kept {
        app.login = login;
        app.say_hello(Auth::Resume { token });
    } else {
        app.login = args.login.clone().unwrap_or_default();
        app.phase = Phase::SignIn;
    }
    Ok(app)
}

/// Where the session token is kept when nothing else is asked for: beside
/// the user's other settings, never in whatever directory the client was
/// started from.
fn session_path() -> PathBuf {
    let home = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = home.join("bota");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("session.txt")
}

/// The login and token the last run left behind.
fn read_session(path: &std::path::Path) -> Option<(String, Token)> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    let login = lines.next()?.trim().to_string();
    let token = lines.next()?.trim().parse::<u128>().ok()?;
    (!login.is_empty()).then_some((login, Token(token)))
}
