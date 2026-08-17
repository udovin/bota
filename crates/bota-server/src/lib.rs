//! Simulation and network server for bota.
//!
//! The simulation lives in [`sim`] and knows nothing about sockets. Everything
//! a participant is allowed to see leaves through `bota-proto`; everything else,
//! including the randomness, stays here.
//!
//! See `DESIGN.md` for the architecture this follows from.

pub mod sim;
