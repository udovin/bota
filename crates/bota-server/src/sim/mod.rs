//! The simulation. Does not know that sockets exist.

mod abilities;
mod arena;
mod blockers;
mod combat;
mod config;
mod econ;
mod ground;
mod items;
mod movement;
mod path;
mod project;
mod rng;
pub mod rules;
mod steer;
mod step;
mod terrain;
mod trees;
mod units;
mod vision;
mod world;

pub use abilities::*;
pub use arena::*;
pub use blockers::*;
pub use combat::*;
pub use config::*;
pub use ground::*;
pub use items::*;
pub use movement::*;
pub use path::*;
pub use rng::*;
pub use steer::*;
pub use step::*;
pub use terrain::*;
pub use trees::*;
pub use units::*;
pub use vision::*;
pub use world::*;

#[cfg(test)]
mod tests;
