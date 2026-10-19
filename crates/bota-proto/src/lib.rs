//! Shared vocabulary and wire format for the bota game protocol.
//!
//! Everything `bota-server`, `bota-client` and `bota-bot` exchange is defined
//! here. A type belongs in this crate when it crosses the wire, or when it is
//! needed to read what crosses the wire.
//!
//! See `DESIGN.md` for the architecture this follows from.

mod attrs;
mod codec;
mod event;
mod game_msg;
mod hall;
mod hub_msg;
mod ids;
mod link;
mod math;
mod msg;
mod names;
mod order;
mod rules;
mod view;

#[cfg(test)]
mod tests;

pub use attrs::*;
pub use codec::*;
pub use event::*;
pub use game_msg::*;
pub use hall::*;
pub use hub_msg::*;
pub use ids::*;
pub use link::*;
pub use math::*;
pub use msg::*;
pub use names::*;
pub use order::*;
pub use rules::*;
pub use view::*;
