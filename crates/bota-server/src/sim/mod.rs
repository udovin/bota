//! The simulation. Does not know that sockets exist.

mod arena;
mod combat;
mod config;
mod econ;
mod movement;
mod project;
mod rng;
pub mod rules;
mod step;
mod units;
mod vision;
mod world;

pub use arena::*;
pub use combat::*;
pub use config::*;
pub use movement::*;
pub use rng::*;
pub use step::*;
pub use units::*;
pub use world::*;

#[cfg(test)]
mod tests;
