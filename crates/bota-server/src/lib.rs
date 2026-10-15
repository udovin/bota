//! Simulation and network server for bota.
//!
//! [`engine`] is how state is kept and walked and knows nothing of Dota;
//! [`game`] is the game itself and knows nothing about sockets. [`hub`] is
//! the always-on part: accounts, the hall, matches between runs and
//! tournaments; [`run`] is one match on its own thread; [`archive`] is what
//! stays on disk. Everything a participant is allowed to see leaves through
//! `bota-proto`; everything else, including the randomness, stays here.
//!
//! See `DESIGN.md` for the architecture this follows from.

pub mod archive;
pub mod engine;
pub mod game;
pub mod hub;
pub mod net;
pub mod replay;
pub mod run;
