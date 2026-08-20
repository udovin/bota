//! What has been put on an entity and runs out on its own.

/// One kind of effect, with what there is of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusKind {
    /// Attacks come faster, by `pct` percent.
    Haste {
        /// Percent taken off the wait between attacks.
        pct: i32,
    },
    /// Health mends faster.
    Mending {
        /// Hundredths of a point a tick.
        per_tick: i32,
    },
    /// Mana mends faster.
    Clarity {
        /// Hundredths of a point a tick.
        per_tick: i32,
    },
    /// Health and mana both mend faster, for standing in a fountain.
    Fountain {
        /// Hundredths of a point of health a tick.
        hp_per_tick: i32,
        /// Hundredths of a point of mana a tick.
        mana_per_tick: i32,
    },
}

/// One effect on an entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    /// What it does, and how much of it there is.
    pub kind: StatusKind,
    /// Ticks before it lifts.
    pub ticks_left: u32,
}

/// Everything on an entity right now. Absent when nothing is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Statuses(pub Vec<Status>);

impl Statuses {
    /// Every effect that has not run out.
    pub fn active(&self) -> impl Iterator<Item = &Status> {
        self.0.iter().filter(|s| s.ticks_left > 0)
    }

    /// Puts one on, in place of whatever of the same kind was already there.
    pub fn put(&mut self, status: Status) {
        let same = std::mem::discriminant(&status.kind);
        self.0
            .retain(|held| std::mem::discriminant(&held.kind) != same);
        self.0.push(status);
    }
}
