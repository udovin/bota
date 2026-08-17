//! Wire format versioning.

/// Version of the wire format, sent in [`ClientMsg::Hello`](crate::ClientMsg).
///
/// Bumped whenever a message is added, removed or reshaped. The server refuses a
/// connection that does not match.
///
/// Says nothing about game balance. That fingerprint is
/// `ruleset_hash`, and it travels in
/// [`ServerMsg::Welcome`](crate::ServerMsg).
pub const PROTO_VERSION: u16 = 1;
