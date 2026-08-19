//! Every neutral creep the jungle spawns, and what each camp holds.
//!
//! Read out of the shipped `npc_units.txt`. Abilities are named there but do
//! nothing here yet.

use crate::sim::{CampKind, rules};

/// One kind of neutral creep.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NeutralKind {
    /// `npc_dota_neutral_kobold`.
    Kobold,
    /// `npc_dota_neutral_kobold_tunneler`.
    KoboldTunneler,
    /// `npc_dota_neutral_kobold_taskmaster`.
    KoboldTaskmaster,
    /// `npc_dota_neutral_forest_troll_berserker`.
    ForestTrollBerserker,
    /// `npc_dota_neutral_forest_troll_high_priest`.
    ForestTrollHighPriest,
    /// `npc_dota_neutral_gnoll_assassin`.
    GnollAssassin,
    /// `npc_dota_neutral_fel_beast`.
    FelBeast,
    /// `npc_dota_neutral_ghost`.
    Ghost,
    /// `npc_dota_neutral_harpy_scout`.
    HarpyScout,
    /// `npc_dota_neutral_harpy_storm`.
    HarpyStorm,
    /// `npc_dota_neutral_centaur_outrunner`.
    CentaurOutrunner,
    /// `npc_dota_neutral_centaur_khan`.
    CentaurKhan,
    /// `npc_dota_neutral_giant_wolf`.
    GiantWolf,
    /// `npc_dota_neutral_alpha_wolf`.
    AlphaWolf,
    /// `npc_dota_neutral_satyr_trickster`.
    SatyrTrickster,
    /// `npc_dota_neutral_satyr_soulstealer`.
    SatyrSoulstealer,
    /// `npc_dota_neutral_satyr_hellcaller`.
    SatyrHellcaller,
    /// `npc_dota_neutral_ogre_mauler`.
    OgreMauler,
    /// `npc_dota_neutral_ogre_magi`.
    OgreMagi,
    /// `npc_dota_neutral_mud_golem`.
    MudGolem,
    /// `npc_dota_neutral_mud_golem_split`.
    MudGolemSplit,
    /// `npc_dota_neutral_polar_furbolg_champion`.
    PolarFurbolgChampion,
    /// `npc_dota_neutral_polar_furbolg_ursa_warrior`.
    PolarFurbolgUrsaWarrior,
    /// `npc_dota_neutral_wildkin`.
    Wildkin,
    /// `npc_dota_neutral_enraged_wildkin`.
    EnragedWildkin,
    /// `npc_dota_neutral_dark_troll`.
    DarkTroll,
    /// `npc_dota_neutral_dark_troll_warlord`.
    DarkTrollWarlord,
    /// `npc_dota_neutral_warpine_raider`.
    WarpineRaider,
    /// `npc_dota_neutral_black_drake`.
    BlackDrake,
    /// `npc_dota_neutral_black_dragon`.
    BlackDragon,
    /// `npc_dota_neutral_rock_golem`.
    RockGolem,
    /// `npc_dota_neutral_granite_golem`.
    GraniteGolem,
    /// `npc_dota_neutral_small_thunder_lizard`.
    SmallThunderLizard,
    /// `npc_dota_neutral_big_thunder_lizard`.
    BigThunderLizard,
    /// `npc_dota_neutral_frostbitten_golem`.
    FrostbittenGolem,
    /// `npc_dota_neutral_ice_shaman`.
    IceShaman,
}

/// The stats of one kind of neutral creep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NeutralDef {
    /// Health at spawn, before upgrades.
    pub hp: i32,
    /// Armor.
    pub armor: i32,
    /// Magic resistance, percent.
    pub magic_resist_pct: i32,
    /// Attack damage, the midpoint of the shipped range.
    pub damage: i32,
    /// Attack range.
    pub attack_range: i32,
    /// How far it looks for a target once awake.
    pub acquisition: i32,
    /// Projectile speed, zero for a melee attack.
    pub projectile_speed: i32,
    /// Ticks between attack starts.
    pub attack_interval: u32,
    /// Ticks from attack start to the hit.
    pub attack_point: u32,
    /// Movement speed, world units per second.
    pub move_speed: i32,
    /// Gold bounty, the midpoint of the shipped range.
    pub bounty: i32,
    /// Experience bounty.
    pub xp: i32,
    /// How fast it turns, in brads per tick.
    pub turn_rate: u16,
    /// Whether it carries the ancient unit type.
    pub ancient: bool,
}

impl NeutralKind {
    /// The stats this kind spawns with.
    pub fn def(self) -> NeutralDef {
        match self {
            NeutralKind::Kobold => NeutralDef {
                hp: 240,
                armor: 0,
                magic_resist_pct: 0,
                damage: 15,
                attack_range: 100,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 11,
                move_speed: 290,
                bounty: 4,
                xp: 14,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::KoboldTunneler => NeutralDef {
                hp: 325,
                armor: 1,
                magic_resist_pct: 0,
                damage: 22,
                attack_range: 100,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 11,
                move_speed: 270,
                bounty: 13,
                xp: 17,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::KoboldTaskmaster => NeutralDef {
                hp: 400,
                armor: 2,
                magic_resist_pct: 0,
                damage: 25,
                attack_range: 110,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 11,
                move_speed: 330,
                bounty: 20,
                xp: 30,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::ForestTrollBerserker => NeutralDef {
                hp: 500,
                armor: 1,
                magic_resist_pct: 0,
                damage: 32,
                attack_range: 500,
                acquisition: 300,
                projectile_speed: 1200,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 270,
                bounty: 19,
                xp: 28,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::ForestTrollHighPriest => NeutralDef {
                hp: 450,
                armor: 0,
                magic_resist_pct: 0,
                damage: 31,
                attack_range: 600,
                acquisition: 300,
                projectile_speed: 900,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 290,
                bounty: 19,
                xp: 28,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::GnollAssassin => NeutralDef {
                hp: 400,
                armor: 1,
                magic_resist_pct: 0,
                damage: 26,
                attack_range: 500,
                acquisition: 800,
                projectile_speed: 1500,
                attack_interval: 60,
                attack_point: 12,
                move_speed: 270,
                bounty: 17,
                xp: 30,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::FelBeast => NeutralDef {
                hp: 400,
                armor: 1,
                magic_resist_pct: 0,
                damage: 14,
                attack_range: 100,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 12,
                move_speed: 350,
                bounty: 17,
                xp: 26,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::Ghost => NeutralDef {
                hp: 500,
                armor: 2,
                magic_resist_pct: 0,
                damage: 40,
                attack_range: 400,
                acquisition: 300,
                projectile_speed: 900,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 320,
                bounty: 24,
                xp: 42,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::HarpyScout => NeutralDef {
                hp: 400,
                armor: 1,
                magic_resist_pct: 0,
                damage: 31,
                attack_range: 300,
                acquisition: 300,
                projectile_speed: 1200,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 280,
                bounty: 15,
                xp: 26,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::HarpyStorm => NeutralDef {
                hp: 500,
                armor: 2,
                magic_resist_pct: 0,
                damage: 33,
                attack_range: 450,
                acquisition: 300,
                projectile_speed: 1200,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 310,
                bounty: 26,
                xp: 42,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::CentaurOutrunner => NeutralDef {
                hp: 350,
                armor: 1,
                magic_resist_pct: 0,
                damage: 19,
                attack_range: 100,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 320,
                bounty: 17,
                xp: 32,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::CentaurKhan => NeutralDef {
                hp: 1100,
                armor: 4,
                magic_resist_pct: 0,
                damage: 52,
                attack_range: 100,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 320,
                bounty: 57,
                xp: 90,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::GiantWolf => NeutralDef {
                hp: 500,
                armor: 1,
                magic_resist_pct: 0,
                damage: 16,
                attack_range: 90,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 10,
                move_speed: 350,
                bounty: 20,
                xp: 40,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::AlphaWolf => NeutralDef {
                hp: 600,
                armor: 3,
                magic_resist_pct: 0,
                damage: 28,
                attack_range: 90,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 10,
                move_speed: 350,
                bounty: 33,
                xp: 60,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::SatyrTrickster => NeutralDef {
                hp: 300,
                armor: 0,
                magic_resist_pct: 0,
                damage: 11,
                attack_range: 280,
                acquisition: 280,
                projectile_speed: 1500,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 300,
                bounty: 13,
                xp: 24,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::SatyrSoulstealer => NeutralDef {
                hp: 600,
                armor: 2,
                magic_resist_pct: 0,
                damage: 22,
                attack_range: 100,
                acquisition: 300,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 270,
                bounty: 20,
                xp: 46,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::SatyrHellcaller => NeutralDef {
                hp: 1100,
                armor: 2,
                magic_resist_pct: 0,
                damage: 52,
                attack_range: 100,
                acquisition: 300,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 290,
                bounty: 63,
                xp: 90,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::OgreMauler => NeutralDef {
                hp: 800,
                armor: 1,
                magic_resist_pct: 0,
                damage: 23,
                attack_range: 100,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 270,
                bounty: 24,
                xp: 32,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::OgreMagi => NeutralDef {
                hp: 600,
                armor: 0,
                magic_resist_pct: 0,
                damage: 19,
                attack_range: 100,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 270,
                bounty: 30,
                xp: 48,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::MudGolem => NeutralDef {
                hp: 750,
                armor: 0,
                magic_resist_pct: 30,
                damage: 25,
                attack_range: 100,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 310,
                bounty: 20,
                xp: 32,
                turn_rate: 5795,
                ancient: false,
            },
            NeutralKind::MudGolemSplit => NeutralDef {
                hp: 250,
                armor: 0,
                magic_resist_pct: 33,
                damage: 12,
                attack_range: 100,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 310,
                bounty: 8,
                xp: 18,
                turn_rate: 5795,
                ancient: false,
            },
            NeutralKind::PolarFurbolgChampion => NeutralDef {
                hp: 700,
                armor: 3,
                magic_resist_pct: 0,
                damage: 41,
                attack_range: 100,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 320,
                bounty: 34,
                xp: 66,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::PolarFurbolgUrsaWarrior => NeutralDef {
                hp: 950,
                armor: 4,
                magic_resist_pct: 0,
                damage: 52,
                attack_range: 100,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 320,
                bounty: 64,
                xp: 90,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::Wildkin => NeutralDef {
                hp: 350,
                armor: 2,
                magic_resist_pct: 0,
                damage: 19,
                attack_range: 128,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 300,
                bounty: 17,
                xp: 26,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::EnragedWildkin => NeutralDef {
                hp: 950,
                armor: 4,
                magic_resist_pct: 0,
                damage: 53,
                attack_range: 128,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 320,
                bounty: 61,
                xp: 90,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::DarkTroll => NeutralDef {
                hp: 500,
                armor: 0,
                magic_resist_pct: 0,
                damage: 25,
                attack_range: 250,
                acquisition: 250,
                projectile_speed: 1200,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 270,
                bounty: 18,
                xp: 42,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::DarkTrollWarlord => NeutralDef {
                hp: 1100,
                armor: 4,
                magic_resist_pct: 0,
                damage: 42,
                attack_range: 250,
                acquisition: 250,
                projectile_speed: 1200,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 300,
                bounty: 43,
                xp: 90,
                turn_rate: 10431,
                ancient: false,
            },
            NeutralKind::WarpineRaider => NeutralDef {
                hp: 850,
                armor: 6,
                magic_resist_pct: 30,
                damage: 40,
                attack_range: 100,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 310,
                bounty: 49,
                xp: 76,
                turn_rate: 5795,
                ancient: false,
            },
            NeutralKind::BlackDrake => NeutralDef {
                hp: 950,
                armor: 2,
                magic_resist_pct: 25,
                damage: 21,
                attack_range: 300,
                acquisition: 300,
                projectile_speed: 900,
                attack_interval: 60,
                attack_point: 15,
                move_speed: 350,
                bounty: 40,
                xp: 95,
                turn_rate: 10431,
                ancient: true,
            },
            NeutralKind::BlackDragon => NeutralDef {
                hp: 2000,
                armor: 4,
                magic_resist_pct: 30,
                damage: 65,
                attack_range: 300,
                acquisition: 300,
                projectile_speed: 1500,
                attack_interval: 60,
                attack_point: 15,
                move_speed: 300,
                bounty: 78,
                xp: 124,
                turn_rate: 10431,
                ancient: true,
            },
            NeutralKind::RockGolem => NeutralDef {
                hp: 800,
                armor: 4,
                magic_resist_pct: 30,
                damage: 23,
                attack_range: 100,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 270,
                bounty: 40,
                xp: 95,
                turn_rate: 5795,
                ancient: true,
            },
            NeutralKind::GraniteGolem => NeutralDef {
                hp: 1500,
                armor: 8,
                magic_resist_pct: 30,
                damage: 82,
                attack_range: 128,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 270,
                bounty: 78,
                xp: 124,
                turn_rate: 5795,
                ancient: true,
            },
            NeutralKind::SmallThunderLizard => NeutralDef {
                hp: 800,
                armor: 3,
                magic_resist_pct: 50,
                damage: 33,
                attack_range: 300,
                acquisition: 800,
                projectile_speed: 1500,
                attack_interval: 54,
                attack_point: 15,
                move_speed: 270,
                bounty: 45,
                xp: 95,
                turn_rate: 5795,
                ancient: true,
            },
            NeutralKind::BigThunderLizard => NeutralDef {
                hp: 1700,
                armor: 3,
                magic_resist_pct: 30,
                damage: 62,
                attack_range: 300,
                acquisition: 300,
                projectile_speed: 1500,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 270,
                bounty: 78,
                xp: 124,
                turn_rate: 10431,
                ancient: true,
            },
            NeutralKind::FrostbittenGolem => NeutralDef {
                hp: 900,
                armor: 7,
                magic_resist_pct: 30,
                damage: 30,
                attack_range: 100,
                acquisition: 500,
                projectile_speed: 0,
                attack_interval: 60,
                attack_point: 9,
                move_speed: 300,
                bounty: 40,
                xp: 95,
                turn_rate: 5795,
                ancient: true,
            },
            NeutralKind::IceShaman => NeutralDef {
                hp: 1500,
                armor: 3,
                magic_resist_pct: 30,
                damage: 60,
                attack_range: 500,
                acquisition: 500,
                projectile_speed: 1500,
                attack_interval: 60,
                attack_point: 21,
                move_speed: 290,
                bounty: 78,
                xp: 124,
                turn_rate: 10431,
                ancient: true,
            },
        }
    }
}

/// One camp roster: the creeps a camp of that size may fill with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Roster {
    /// Which size of camp draws it.
    pub kind: CampKind,
    /// The creeps it spawns.
    pub creeps: &'static [NeutralKind],
}

/// Every roster in the jungle, grouped by camp size.
pub const ROSTERS: [Roster; 21] = [
    // Kobold Camp
    Roster {
        kind: CampKind::Small,
        creeps: &[
            NeutralKind::Kobold,
            NeutralKind::Kobold,
            NeutralKind::Kobold,
            NeutralKind::KoboldTunneler,
            NeutralKind::KoboldTaskmaster,
        ],
    },
    // Hill Troll Camp
    Roster {
        kind: CampKind::Small,
        creeps: &[
            NeutralKind::ForestTrollBerserker,
            NeutralKind::ForestTrollBerserker,
            NeutralKind::ForestTrollHighPriest,
        ],
    },
    // Hill Troll and Kobold Camp
    Roster {
        kind: CampKind::Small,
        creeps: &[
            NeutralKind::ForestTrollBerserker,
            NeutralKind::ForestTrollBerserker,
            NeutralKind::KoboldTaskmaster,
        ],
    },
    // Vhoul Assassin Camp
    Roster {
        kind: CampKind::Small,
        creeps: &[
            NeutralKind::GnollAssassin,
            NeutralKind::GnollAssassin,
            NeutralKind::GnollAssassin,
        ],
    },
    // Ghost Camp
    Roster {
        kind: CampKind::Small,
        creeps: &[
            NeutralKind::FelBeast,
            NeutralKind::FelBeast,
            NeutralKind::Ghost,
        ],
    },
    // Harpy Camp
    Roster {
        kind: CampKind::Small,
        creeps: &[
            NeutralKind::HarpyScout,
            NeutralKind::HarpyScout,
            NeutralKind::HarpyStorm,
        ],
    },
    // Centaur Camp
    Roster {
        kind: CampKind::Medium,
        creeps: &[NeutralKind::CentaurOutrunner, NeutralKind::CentaurKhan],
    },
    // Wolf Camp
    Roster {
        kind: CampKind::Medium,
        creeps: &[
            NeutralKind::GiantWolf,
            NeutralKind::GiantWolf,
            NeutralKind::AlphaWolf,
        ],
    },
    // Satyr Camp
    Roster {
        kind: CampKind::Medium,
        creeps: &[
            NeutralKind::SatyrTrickster,
            NeutralKind::SatyrTrickster,
            NeutralKind::SatyrSoulstealer,
            NeutralKind::SatyrSoulstealer,
        ],
    },
    // Ogre Camp
    Roster {
        kind: CampKind::Medium,
        creeps: &[
            NeutralKind::OgreMauler,
            NeutralKind::OgreMauler,
            NeutralKind::OgreMagi,
        ],
    },
    // Golem Camp
    Roster {
        kind: CampKind::Medium,
        creeps: &[NeutralKind::MudGolem, NeutralKind::MudGolem],
    },
    // Large Centaur Camp
    Roster {
        kind: CampKind::Large,
        creeps: &[
            NeutralKind::CentaurOutrunner,
            NeutralKind::CentaurOutrunner,
            NeutralKind::CentaurKhan,
        ],
    },
    // Large Satyr Camp
    Roster {
        kind: CampKind::Large,
        creeps: &[
            NeutralKind::SatyrTrickster,
            NeutralKind::SatyrSoulstealer,
            NeutralKind::SatyrHellcaller,
        ],
    },
    // Hellbear Camp
    Roster {
        kind: CampKind::Large,
        creeps: &[
            NeutralKind::PolarFurbolgChampion,
            NeutralKind::PolarFurbolgUrsaWarrior,
        ],
    },
    // Wildwing Camp
    Roster {
        kind: CampKind::Large,
        creeps: &[
            NeutralKind::Wildkin,
            NeutralKind::Wildkin,
            NeutralKind::EnragedWildkin,
        ],
    },
    // Troll Camp
    Roster {
        kind: CampKind::Large,
        creeps: &[
            NeutralKind::DarkTroll,
            NeutralKind::DarkTroll,
            NeutralKind::DarkTrollWarlord,
        ],
    },
    // Warpine Camp
    Roster {
        kind: CampKind::Large,
        creeps: &[NeutralKind::WarpineRaider, NeutralKind::WarpineRaider],
    },
    // Dragon Camp
    Roster {
        kind: CampKind::Ancient,
        creeps: &[
            NeutralKind::BlackDrake,
            NeutralKind::BlackDrake,
            NeutralKind::BlackDragon,
        ],
    },
    // Large Golem Camp
    Roster {
        kind: CampKind::Ancient,
        creeps: &[
            NeutralKind::RockGolem,
            NeutralKind::RockGolem,
            NeutralKind::GraniteGolem,
        ],
    },
    // Thunderhide Camp
    Roster {
        kind: CampKind::Ancient,
        creeps: &[
            NeutralKind::SmallThunderLizard,
            NeutralKind::SmallThunderLizard,
            NeutralKind::BigThunderLizard,
        ],
    },
    // Frostbitten Camp
    Roster {
        kind: CampKind::Ancient,
        creeps: &[
            NeutralKind::FrostbittenGolem,
            NeutralKind::FrostbittenGolem,
            NeutralKind::IceShaman,
        ],
    },
];

/// The rosters a camp of this size draws from.
pub fn rosters_of(kind: CampKind) -> impl Iterator<Item = (u8, &'static Roster)> {
    ROSTERS
        .iter()
        .enumerate()
        .filter(move |(_, r)| r.kind == kind)
        .map(|(i, r)| (i as u8, r))
}

/// Health, armor, damage and bounty a neutral gains per upgrade interval.
pub const fn upgraded(def: NeutralDef, upgrades: i32) -> NeutralDef {
    NeutralDef {
        hp: def.hp + rules::NEUTRAL_UPGRADE_HP * upgrades,
        armor: def.armor + rules::NEUTRAL_UPGRADE_ARMOR_HALVES * upgrades / 2,
        damage: def.damage + rules::NEUTRAL_UPGRADE_DAMAGE * upgrades,
        bounty: def.bounty + rules::NEUTRAL_UPGRADE_GOLD * upgrades,
        xp: def.xp + rules::NEUTRAL_UPGRADE_XP * upgrades,
        ..def
    }
}
