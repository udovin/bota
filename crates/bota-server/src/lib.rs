//! Simulation and network servers for bota.
//!
//! [`engine`] is how state is kept and walked and knows nothing of Dota;
//! [`game`] is the game itself and knows nothing about sockets. [`hub`] is
//! the always-on part: accounts, the hall, matches before and after they run,
//! tournaments and the archive. [`host`] is a game server: it runs the
//! matches the hub gives it, each one in [`run`] on its own thread, for the
//! players the hub lets in. [`archive`] is what stays on disk, and [`keys`]
//! the secrets. Everything a participant is allowed to see leaves through
//! `bota-proto`; everything else, including the randomness, stays here.
//!
//! See `DESIGN.md` for the architecture this follows from.

pub mod archive;
pub mod commands;
pub mod engine;
pub mod game;
pub mod host;
pub mod hub;
pub mod keys;
pub mod net;
pub mod replay;
pub mod run;
