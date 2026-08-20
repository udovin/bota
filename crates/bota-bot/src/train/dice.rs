//! A small deterministic source of numbers, for the search alone.
//!
//! Nothing here reaches the simulation: a search draws from it to decide which
//! number to move and how far, and the same seed walks the same path. The
//! server's own randomness is its business and is seeded separately.

/// A stream of numbers from one seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dice {
    /// Where the stream stands.
    state: u64,
}

impl Dice {
    /// A stream from a seed.
    pub fn from_seed(seed: u64) -> Dice {
        Dice {
            state: seed ^ 0x9e37_79b9_7f4a_7c15,
        }
    }

    /// The next number of the stream.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number from zero up to one, the one not included.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u32 << 24) as f32
    }

    /// A number about zero, most of them within one of it.
    pub fn spread(&mut self) -> f32 {
        let one = self.unit().max(f32::MIN_POSITIVE);
        let other = self.unit();
        (-2.0 * one.ln()).sqrt() * (std::f32::consts::TAU * other).cos()
    }

    /// One of the first `count` numbers, or zero when there are none.
    pub fn below(&mut self, count: usize) -> usize {
        if count == 0 {
            return 0;
        }
        (self.next_u64() % count as u64) as usize
    }
}
