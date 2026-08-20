//! A bot that plays a hero, and what it takes to train one.
//!
//! It joins a server the way any client does, is told only what its own side
//! may see, and answers with orders. What it decides lives in the brain and
//! knows nothing about sockets; what carries the decisions is the link; the
//! numbers it decides by are held apart from the decisions themselves, so a
//! search can play one set against another.

mod brain;
mod geom;
mod link;
mod params;
mod session;
mod trail;
mod train;

pub use brain::*;
pub use geom::*;
pub use link::*;
pub use params::*;
pub use session::*;
pub use trail::*;
pub use train::*;

#[cfg(test)]
mod tests;
