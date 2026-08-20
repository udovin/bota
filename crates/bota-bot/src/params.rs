//! The numbers the bot plays by, in one place.
//!
//! Every threshold the policy weighs is a field here rather than a constant at
//! the place that reads it: a run is handed one set, a search walks over them,
//! and a file keeps the set that played best. Each field carries the range a
//! search may move it in.
//!
//! Some of them stand for what the wire does not carry: how long a swing takes
//! before it lands, how fast an arrow flies, what an ability reaches. The bot
//! holds those as numbers like any other here, and they are tuned the same way.

use std::fmt;
use std::path::{Path, PathBuf};

/// Declares the whole set once: the fields, their defaults, and the range a
/// search may move each one in.
macro_rules! knobs {
    ($($(#[$doc:meta])* $name:ident = $default:expr, $low:expr, $high:expr;)*) => {
        /// The numbers one bot plays by.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct Params {
            $($(#[$doc])* pub $name: f32,)*
        }

        impl Default for Params {
            fn default() -> Params {
                Params { $($name: $default,)* }
            }
        }

        impl Params {
            /// What each field is called, in the order the numbers come out.
            pub const NAMES: &'static [&'static str] = &[$(stringify!($name),)*];

            /// The range each field may be moved in, in the same order.
            pub const RANGES: &'static [(f32, f32)] = &[$(($low, $high),)*];

            /// The numbers themselves, in the order of [`Params::NAMES`].
            pub fn to_vec(&self) -> Vec<f32> {
                vec![$(self.$name,)*]
            }

            /// A set built from numbers in that order. A short list leaves the
            /// rest at their defaults.
            pub fn from_slice(values: &[f32]) -> Params {
                let mut left = values.iter().copied();
                Params { $($name: left.next().unwrap_or($default),)* }
            }

            /// Puts one number in by name, answering whether the name is one of
            /// ours.
            pub fn set_named(&mut self, name: &str, value: f32) -> bool {
                match name {
                    $(stringify!($name) => { self.$name = value; true })*
                    _ => false,
                }
            }
        }
    };
}

knobs! {
    /// Ticks between the start of a swing and the moment it lands.
    swing_lead_ticks = 9.0, 0.0, 30.0;
    /// Ticks a swing waits on coming round, for a target square behind it.
    turn_lead_ticks = 8.0, 0.0, 60.0;
    /// World units an arrow covers in a second.
    missile_speed = 900.0, 200.0, 2000.0;
    /// What one point of armor adds to the hundred a blow is divided by.
    armor_scale = 6.0, 1.0, 20.0;

    /// Reach beyond its own that it will step into for a last hit.
    last_hit_slack = 250.0, 0.0, 1200.0;
    /// Part of its whole health at which one of its own may be denied.
    deny_hp_part = 0.5, 0.0, 1.0;
    /// How much of the damage already aimed at a creep it counts on landing
    /// before its own swing does.
    incoming_weight = 1.0, 0.0, 2.0;
    /// Health a creep may be left standing with for a swing to still be worth
    /// taking.
    last_hit_margin = 0.0, -100.0, 100.0;

    /// Part of its whole health below which it leaves the lane.
    retreat_hp_part = 0.3, 0.0, 1.0;
    /// Part of its whole health at which it comes back out.
    return_hp_part = 0.75, 0.0, 1.0;
    /// Part of its whole health below which it drinks what it carries.
    mend_hp_part = 0.65, 0.0, 1.0;
    /// Seconds of the damage aimed at it that it will not stand through.
    dread_seconds = 2.5, 0.0, 20.0;
    /// Ground it keeps between itself and the edge of an enemy tower's reach.
    tower_clearance = 150.0, 0.0, 1500.0;
    /// How far back down the lane a retreat takes it.
    fall_back = 1600.0, 200.0, 8000.0;
    /// Part of its whole health below which it goes all the way home.
    go_home_part = 0.15, 0.0, 1.0;

    /// How far behind the front of the wave it stands.
    stand_off = 250.0, 0.0, 1500.0;
    /// How far from the front of the other side's wave it stands, as a part of
    /// its own reach. Below one it stands where it can swing.
    keep_off_part = 0.85, 0.1, 1.5;
    /// How far off a lane's line a building still belongs to that lane.
    lane_width = 2200.0, 500.0, 6000.0;
    /// How near a spot counts as reached.
    arrive_radius = 120.0, 16.0, 800.0;
    /// How far a destination has to have moved before a walk order is renewed.
    resend_drift = 200.0, 16.0, 2000.0;
    /// Ticks before the same want is put on the wire again.
    resend_ticks = 6.0, 1.0, 90.0;

    /// Health advantage, as a part of its whole, before it goes at a hero.
    harass_edge = 0.15, -1.0, 1.0;
    /// How far an enemy may be and still be worth a spell.
    cast_reach = 600.0, 100.0, 1500.0;
    /// How far a hook is thrown.
    hook_reach = 1000.0, 200.0, 1500.0;
    /// Ticks of lead taken when a hook is aimed at somebody walking.
    hook_lead_ticks = 12.0, 0.0, 60.0;
    /// How near the rot has to catch somebody before it is switched on.
    rot_reach = 250.0, 50.0, 800.0;
    /// Enemies within reach before the ultimate that hits everything is worth
    /// spending.
    volley_needs = 2.0, 1.0, 10.0;
    /// Part of its mana it keeps back from everything but the ultimate.
    mana_floor_part = 0.2, 0.0, 1.0;

    /// Ticks of standing still, while meaning to walk, before it works itself
    /// loose.
    wedge_ticks = 12.0, 2.0, 300.0;
    /// World units in a tick that count as having moved at all.
    wedge_step = 4.0, 0.0, 100.0;
    /// How far to the side it steps to work itself loose.
    detour_reach = 700.0, 100.0, 3000.0;
    /// Ticks it keeps to a detour before asking again.
    detour_ticks = 20.0, 2.0, 300.0;

    /// How far a tree may be and still be within reach of a tango.
    tree_reach = 165.0, 50.0, 500.0;

    /// Things waiting in the stash before the courier is sent for them.
    courier_batch = 2.0, 1.0, 6.0;
    /// Ticks the first thing waits in the stash before it is sent for
    /// whatever else has turned up by then.
    courier_patience = 300.0, 0.0, 3000.0;
    /// Damage a tick landing on the bot above which the courier is kept away
    /// from it. Nothing at all by default: a courier walks to where its owner
    /// stands, and whatever is shooting its owner is what it walks into.
    courier_dread = 0.0, 0.0, 20.0;
    /// How far the courier has to be from the bot before its turn of speed is
    /// worth spending.
    burst_gap = 4000.0, 0.0, 18000.0;
    /// Ticks before an errand already given is given again.
    ///
    /// An errand outlives the tick it was given in, so saying it twice buys
    /// nothing and costs the tick the bot would have swung in. Saying it again
    /// eventually is only in case it never arrived.
    courier_repeat = 300.0, 30.0, 3000.0;
}

/// What the kept file says about itself, above the numbers.
pub const KEPT_HEADING: &str =
    "# The numbers this bot plays by, as the last training run left them.
#
# Written by `bota-bot train`, baked into the bot when it is built. A name left
# out keeps the number the code was written with; a name nothing is called is
# an error.
";

impl Params {
    /// Where a trained set is kept: beside the repository, not inside it.
    ///
    /// Worked out from the tree this was built from. A set of numbers is what
    /// one machine's training run happened to arrive at, so it is neither
    /// committed nor carried inside the binary; a build without one plays by
    /// the numbers the code was written with.
    pub fn path() -> PathBuf {
        beside_the_repository("params.txt")
    }

    /// What the last training run left, read from that file.
    ///
    /// Read once and kept: this is asked for every time a bot is made, and
    /// training makes thousands of them.
    pub fn learned() -> Params {
        static KEPT: std::sync::OnceLock<Params> = std::sync::OnceLock::new();
        *KEPT.get_or_init(|| Params::read_from(&Params::path()).unwrap_or_default())
    }

    /// The set a file holds, or nothing when it holds none that reads.
    pub fn read_from(path: &Path) -> Option<Params> {
        let text = std::fs::read_to_string(path).ok()?;
        Params::parse(&text).ok()
    }

    /// How many numbers there are.
    pub fn count() -> usize {
        Params::NAMES.len()
    }

    /// The set as it is written to a file: one `name = number` a line, under
    /// the heading that says what the file is.
    ///
    /// Training rewrites the file whole, so the heading is written with it or
    /// it is lost the first time the numbers improve.
    pub fn to_text(&self) -> String {
        let mut out = String::from(KEPT_HEADING);
        for (name, value) in Params::NAMES.iter().zip(self.to_vec()) {
            out.push_str(name);
            out.push_str(" = ");
            out.push_str(&value.to_string());
            out.push('\n');
        }
        out
    }

    /// A set read back from that form.
    ///
    /// Blank lines and anything after a `#` are skipped, a name that is not one
    /// of ours is an error, and a name left out keeps its default.
    pub fn parse(text: &str) -> Result<Params, ParamError> {
        let mut params = Params::default();
        for (index, whole) in text.lines().enumerate() {
            let line = whole.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let at = index + 1;
            let Some((name, value)) = line.split_once('=') else {
                return Err(ParamError::NotAPair { line: at });
            };
            let name = name.trim().to_string();
            let Ok(value) = value.trim().parse::<f32>() else {
                return Err(ParamError::NotANumber { line: at, name });
            };
            if !params.set_named(&name, value) {
                return Err(ParamError::NoSuchName { line: at, name });
            }
        }
        Ok(params.clamped())
    }

    /// The same set with every number brought inside its range.
    ///
    /// A number that is not finite is put at the low end.
    pub fn clamped(&self) -> Params {
        let brought: Vec<f32> = self
            .to_vec()
            .iter()
            .zip(Params::RANGES)
            .map(|(value, (low, high))| {
                if value.is_finite() {
                    value.clamp(*low, *high)
                } else {
                    *low
                }
            })
            .collect();
        Params::from_slice(&brought)
    }
}

/// Why a written set would not read back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParamError {
    /// A line that is neither blank nor a name and a number.
    NotAPair {
        /// Which line, counting from one.
        line: usize,
    },
    /// A line whose value is not a number.
    NotANumber {
        /// Which line, counting from one.
        line: usize,
        /// The name it was given for.
        name: String,
    },
    /// A line naming something the bot does not play by.
    NoSuchName {
        /// Which line, counting from one.
        line: usize,
        /// The name that was given.
        name: String,
    },
}

impl fmt::Display for ParamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParamError::NotAPair { line } => write!(f, "line {line} is not a name and a number"),
            ParamError::NotANumber { line, name } => {
                write!(f, "line {line}: {name} was not given a number")
            }
            ParamError::NoSuchName { line, name } => {
                write!(f, "line {line}: nothing is called {name}")
            }
        }
    }
}

impl std::error::Error for ParamError {}

/// One of the files that sit beside the repository rather than inside it.
///
/// The tree this was built from, with the crate directories taken back off.
/// Written out plainly so that the help text a binary prints names a path a
/// person can read.
pub fn beside_the_repository(named: &str) -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    here.parent()
        .and_then(Path::parent)
        .unwrap_or(here)
        .join(named)
}
