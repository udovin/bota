//! Where a seat's ticks come from.
//!
//! A seat reads what the match has to say, asks for one thing a tick, and says
//! when it has finished thinking. That is the whole of what it needs, and it is
//! the same three questions whether the match is on the other end of a socket
//! or in this very process.
//!
//! One trait, so there is one seat loop. Two loops, one a copy of the other,
//! would part company by the second change to either, and then the model would
//! be trained on one game and played on another.

use bota_proto::ServerMsg;

use crate::Ask;

/// Whatever a seat reads its match from.
pub trait Wire {
    /// The next thing the match has to say. Nothing once it is over.
    fn hear(&mut self) -> std::io::Result<Option<ServerMsg>>;

    /// Asks for something. One a tick; a second replaces the first.
    fn order(&mut self, ask: Ask) -> std::io::Result<()>;

    /// Says this seat has finished thinking about a tick.
    fn done_thinking(&mut self, tick: u32) -> std::io::Result<()>;
}

impl Wire for crate::Link {
    fn hear(&mut self) -> std::io::Result<Option<ServerMsg>> {
        crate::Link::hear(self)
    }

    fn order(&mut self, ask: Ask) -> std::io::Result<()> {
        crate::Link::order(self, ask)
    }

    fn done_thinking(&mut self, tick: u32) -> std::io::Result<()> {
        crate::Link::done_thinking(self, tick)
    }
}
