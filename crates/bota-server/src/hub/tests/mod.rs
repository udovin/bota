//! The hub speaks the protocol end to end over real sockets; what a command
//! line asks for is also read on its own.

mod abandoning;
mod archiving;
mod auth;
mod chat;
mod crews;
mod fixtures;
mod matching;
mod picking;
mod queueing;
mod reading;
mod servers;
mod tournaments;
