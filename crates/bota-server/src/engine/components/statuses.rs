//! What has been put on an entity and runs out on its own.

/// One kind of effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusKind {
    /// Attacks come faster, by `magnitude` percent.
    Haste,
    /// Health mends faster, by `magnitude` a tick.
    Mending,
    /// Mana mends faster, by `magnitude` a tick.
    Clarity,
}

/// One effect on an entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    /// What it does.
    pub kind: StatusKind,
    /// Ticks before it lifts.
    pub ticks_left: u32,
    /// How much of it there is, read against [`StatusKind`].
    pub magnitude: i32,
}

/// Everything on an entity right now. Absent when nothing is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Statuses(pub Vec<Status>);

impl Statuses {
    /// Every effect that has not run out.
    pub fn active(&self) -> impl Iterator<Item = &Status> {
        self.0.iter().filter(|s| s.ticks_left > 0)
    }
}
