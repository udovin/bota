//! The simulation. Does not know that sockets exist.

mod abilities;
mod arena;
mod blockers;
mod combat;
mod config;
mod creep;
mod econ;
mod ground;
mod items;
mod map;
mod movement;
mod path;
mod project;
mod rng;
pub mod rules;
mod separate;
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
pub use creep::*;
pub use ground::*;
pub use items::*;
pub use map::*;
pub use movement::*;
pub use path::*;
pub use rng::*;
pub use separate::*;
pub use step::*;
pub use terrain::*;
pub use trees::*;
pub use units::*;
pub use vision::*;
pub use world::*;

#[cfg(test)]
mod tests;
