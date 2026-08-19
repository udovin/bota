//! Who can see what.

use bota_proto::Team;

/// Which sides see an entity right now, one bit a side.
///
/// Written afresh each tick by the system that works out sight, and read by
/// everything that has to know what a side is allowed to be told. Nothing else
/// writes here: a value put in by hand is gone next tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Visibility(u8);

impl Visibility {
    /// Seen by nobody.
    pub const NONE: Visibility = Visibility(0);

    /// The bit a side occupies.
    const fn bit(team: Team) -> u8 {
        1 << (team as u8)
    }

    /// Whether a side sees the entity.
    pub fn by(self, team: Team) -> bool {
        self.0 & Visibility::bit(team) != 0
    }

    /// Records that a side sees the entity. Naming one twice changes nothing.
    pub fn add(&mut self, team: Team) {
        self.0 |= Visibility::bit(team);
    }

    /// Forgets every side.
    pub fn clear(&mut self) {
        self.0 = 0;
    }

    /// Whether nobody sees it.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The bits themselves, for the world fingerprint.
    pub fn bits(self) -> u8 {
        self.0
    }
}
