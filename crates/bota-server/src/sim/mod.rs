//! The simulation. Does not know that sockets exist.

mod arena;
mod rng;

pub use arena::*;
pub use rng::*;

#[cfg(test)]
mod tests;
