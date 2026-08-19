//! How far a neutral may be dragged, and how it lets go.

use bota_proto::Vec2;

/// A neutral creep's aggro window, leash and way home.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NeutralAi {
    /// Which camp of [`crate::sim::CAMPS`] it belongs to.
    pub camp: u8,
    /// The exact spot it spawned on, which is what the guard distance is
    /// measured from and where it walks back to.
    pub home: Vec2,
    /// Ticks left of the aggro window while it is beyond the guard distance.
    /// Zero while inside it.
    pub leash_left: u32,
    /// Ticks in which damage cannot wake it again after a leash break.
    pub reaggro_block: u32,
    /// Length of the window the next aggro grants, in ticks. Five seconds
    /// normally, three after an early re-aggro.
    pub next_window: u32,
    /// Walking home: deaf to anyone merely standing close, still awake to
    /// damage, and still swinging at whatever comes into range on the way.
    pub going_home: bool,
}

impl NeutralAi {
    /// A neutral fresh in its camp, asleep.
    pub fn new(camp: u8, home: Vec2, full_window: u32) -> NeutralAi {
        NeutralAi {
            camp,
            home,
            leash_left: 0,
            reaggro_block: 0,
            next_window: full_window,
            going_home: false,
        }
    }
}
