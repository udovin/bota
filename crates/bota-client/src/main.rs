//! bota client: rendering, input, spectating and replays.
//!
//! The client draws what the server sends and sends back intents. It holds no
//! `World` and cannot: the simulation lives in a crate this one does not
//! depend on.

mod camera;
mod hud;
mod icons;
mod input;
mod net;
mod render;
mod replay_play;
mod state;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use bota_proto::{ClientMsg, Role};
use macroquad::prelude::{Conf, next_frame};

use crate::net::Net;
use crate::replay_play::ReplayPlayer;
use crate::state::{App, Source};

/// What the command line asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Args {
    /// Server address to connect to.
    pub addr: String,
    /// Display name for the lobby.
    pub name: String,
    /// Watch instead of playing.
    pub spectate: bool,
    /// Play this file instead of connecting anywhere.
    pub replay: Option<PathBuf>,
}

/// Parses arguments; `None` asks for the usage text.
pub fn parse_args(args: &[String]) -> Option<Args> {
    let mut parsed = Args {
        addr: "127.0.0.1:4455".to_string(),
        name: "player".to_string(),
        spectate: false,
        replay: None,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--addr" => parsed.addr = it.next()?.clone(),
            "--name" => parsed.name = it.next()?.clone(),
            "--spectate" => parsed.spectate = true,
            "--replay" => parsed.replay = Some(PathBuf::from(it.next()?)),
            _ => return None,
        }
    }
    Some(parsed)
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
    let cli: Vec<String> = std::env::args().skip(1).collect();
    let Some(args) = parse_args(&cli) else {
        eprintln!(
            "usage: bota-client [--addr HOST:PORT] [--name NAME] [--spectate] [--replay FILE]"
        );
        return;
    };
    let mut app = match build_app(&args) {
        Ok(app) => app,
        Err(err) => {
            eprintln!("bota-client: {err}");
            return;
        }
    };
    loop {
        let dt = macroquad::prelude::get_frame_time();
        let msgs = app.source.poll(dt);
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

fn build_app(args: &Args) -> std::io::Result<App> {
    if let Some(path) = &args.replay {
        let player = ReplayPlayer::load(path)?;
        return Ok(App::new(Source::Replay(player)));
    }
    let mut net = Net::connect(&args.addr)?;
    let role = if args.spectate {
        Role::Spectator
    } else {
        Role::Player
    };
    net.send(&ClientMsg::Hello {
        role,
        name: args.name.clone(),
    });
    Ok(App::new(Source::Live(net)))
}
