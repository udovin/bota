//! Balance constants.
//!
//! Everything here is in ticks, whole world units or plain integers. Time
//! constants assume [`TICKS_PER_SECOND`]; the wall-clock pace of a match is a
//! server option and does not change them.

use bota_proto::{Fixed, Vec2};

/// Simulation ticks per second of game time.
pub const TICKS_PER_SECOND: u32 = 30;

/// The map spans `0..MAP_SIZE` on both axes.
pub const MAP_SIZE: i32 = 8192;

/// Cells per axis of the passability grid.
pub const GRID_CELLS: usize = 128;

/// World units covered by one passability cell.
pub const GRID_CELL_SIZE: i32 = MAP_SIZE / GRID_CELLS as i32;

// Landmarks. The map is symmetric across the main diagonal; every Dire
// position is the Radiant one mirrored through the map center.

/// Center of the Radiant fountain area.
pub const RADIANT_FOUNTAIN_POS: Vec2 = Vec2::from_ints(256, 256);
/// Center of the Dire fountain area.
pub const DIRE_FOUNTAIN_POS: Vec2 = Vec2::from_ints(MAP_SIZE - 256, MAP_SIZE - 256);
/// The Radiant Ancient.
pub const RADIANT_ANCIENT_POS: Vec2 = Vec2::from_ints(768, 768);
/// The Dire Ancient.
pub const DIRE_ANCIENT_POS: Vec2 = Vec2::from_ints(MAP_SIZE - 768, MAP_SIZE - 768);
/// The Radiant mid tower.
pub const RADIANT_TOWER_POS: Vec2 = Vec2::from_ints(2304, 2304);
/// The Dire mid tower.
pub const DIRE_TOWER_POS: Vec2 = Vec2::from_ints(MAP_SIZE - 2304, MAP_SIZE - 2304);
/// How far from the fountain center a hero appears, along both axes towards
/// the map center. Keeps the spawn clear of the fountain's collision radius.
pub const HERO_SPAWN_OFFSET: i32 = 140;
/// Where Radiant creep waves appear.
pub const RADIANT_CREEP_SPAWN: Vec2 = Vec2::from_ints(1152, 1152);
/// Where Dire creep waves appear.
pub const DIRE_CREEP_SPAWN: Vec2 = Vec2::from_ints(MAP_SIZE - 1152, MAP_SIZE - 1152);

// Creep waves.

/// Tick of the first creep wave.
pub const FIRST_WAVE_TICK: u32 = 90;
/// Ticks between creep waves.
pub const WAVE_PERIOD_TICKS: u32 = 900;
/// Every n-th wave brings a siege creep.
pub const SIEGE_WAVE_PERIOD: u32 = 5;
/// Melee creeps per wave.
pub const MELEE_PER_WAVE: u32 = 3;
/// Offsets of wave members around the spawn point, one per creep.
pub const WAVE_SPAWN_OFFSETS: [Vec2; 5] = [
    Vec2::from_ints(-48, 48),
    Vec2::from_ints(0, 0),
    Vec2::from_ints(48, -48),
    Vec2::from_ints(-32, -32),
    Vec2::from_ints(64, 64),
];

// Generic hero stats. Per-hero data replaces these when heroes arrive.

/// Hero health at level one.
pub const HERO_HP: i32 = 620;
/// Hero mana at level one.
pub const HERO_MANA: i32 = 300;
/// Hero movement speed, world units per second.
pub const HERO_MOVE_SPEED: i32 = 300;
/// Hero attack damage at level one.
pub const HERO_ATTACK_DAMAGE: i32 = 55;
/// Hero attack range.
pub const HERO_ATTACK_RANGE: i32 = 600;
/// Ticks between hero attack starts.
pub const HERO_ATTACK_INTERVAL: u32 = 51;
/// Ticks from attack start to the projectile leaving.
pub const HERO_ATTACK_POINT: u32 = 9;
/// Hero attack projectile speed, world units per second.
pub const HERO_PROJECTILE_SPEED: i32 = 900;
/// Hero armor.
pub const HERO_ARMOR: i32 = 3;
/// Hero magic resistance, percent.
pub const HERO_MAGIC_RESIST_PCT: i32 = 25;
/// Hero collision radius.
pub const HERO_RADIUS: i32 = 24;
/// Hero fog light radius.
pub const HERO_VISION: i32 = 1800;
/// Extra health per level past the first.
pub const HERO_HP_PER_LEVEL: i32 = 90;
/// Extra mana per level past the first.
pub const HERO_MANA_PER_LEVEL: i32 = 30;
/// Extra attack damage per level past the first.
pub const HERO_ATTACK_DAMAGE_PER_LEVEL: i32 = 6;
/// A hero regains one health point every this many ticks.
pub const HERO_HP_REGEN_PERIOD: u32 = 20;
/// A hero regains one mana point every this many ticks.
pub const HERO_MANA_REGEN_PERIOD: u32 = 15;
/// Highest hero level.
pub const HERO_MAX_LEVEL: u8 = 10;
/// Total experience required to sit at each level, indexed by `level - 1`.
pub const XP_THRESHOLDS: [i32; HERO_MAX_LEVEL as usize] =
    [0, 230, 600, 1080, 1660, 2260, 2980, 3730, 4620, 5550];
/// Ticks a dead hero waits before respawning, plus the per-level term.
pub const RESPAWN_BASE_TICKS: u32 = 120;
/// Additional respawn ticks per hero level.
pub const RESPAWN_PER_LEVEL_TICKS: u32 = 120;

// Creep stats: melee, ranged, siege.

/// Melee creep health.
pub const MELEE_CREEP_HP: i32 = 550;
/// Melee creep attack damage.
pub const MELEE_CREEP_ATTACK_DAMAGE: i32 = 21;
/// Melee creep attack range.
pub const MELEE_CREEP_ATTACK_RANGE: i32 = 100;
/// Melee creep gold bounty.
pub const MELEE_CREEP_BOUNTY: i32 = 36;
/// Melee creep experience.
pub const MELEE_CREEP_XP: i32 = 40;

/// Ranged creep health.
pub const RANGED_CREEP_HP: i32 = 300;
/// Ranged creep attack damage.
pub const RANGED_CREEP_ATTACK_DAMAGE: i32 = 24;
/// Ranged creep attack range.
pub const RANGED_CREEP_ATTACK_RANGE: i32 = 500;
/// Ranged creep gold bounty.
pub const RANGED_CREEP_BOUNTY: i32 = 51;
/// Ranged creep experience.
pub const RANGED_CREEP_XP: i32 = 66;

/// Siege creep health.
pub const SIEGE_CREEP_HP: i32 = 875;
/// Siege creep attack damage.
pub const SIEGE_CREEP_ATTACK_DAMAGE: i32 = 40;
/// Siege creep attack range.
pub const SIEGE_CREEP_ATTACK_RANGE: i32 = 690;
/// Siege creep armor.
pub const SIEGE_CREEP_ARMOR: i32 = 10;
/// Siege creep gold bounty.
pub const SIEGE_CREEP_BOUNTY: i32 = 74;
/// Siege creep experience.
pub const SIEGE_CREEP_XP: i32 = 88;
/// Ticks between siege creep attack starts.
pub const SIEGE_CREEP_ATTACK_INTERVAL: u32 = 80;

/// Creep movement speed, world units per second.
pub const CREEP_MOVE_SPEED: i32 = 325;
/// Ticks between creep attack starts.
pub const CREEP_ATTACK_INTERVAL: u32 = 30;
/// Ticks from creep attack start to the hit or the projectile leaving.
pub const CREEP_ATTACK_POINT: u32 = 8;
/// Melee creep armor.
pub const MELEE_CREEP_ARMOR: i32 = 2;
/// Creep collision radius.
pub const CREEP_RADIUS: i32 = 16;
/// Creep fog light radius.
pub const CREEP_VISION: i32 = 850;
/// Creep and siege attack projectile speed, world units per second.
pub const CREEP_PROJECTILE_SPEED: i32 = 900;

// Buildings.

/// Tower health.
pub const TOWER_HP: i32 = 1800;
/// Tower attack damage.
pub const TOWER_ATTACK_DAMAGE: i32 = 110;
/// Tower attack range.
pub const TOWER_ATTACK_RANGE: i32 = 700;
/// Ticks between tower attack starts.
pub const TOWER_ATTACK_INTERVAL: u32 = 29;
/// Ticks from tower attack start to the projectile leaving.
pub const TOWER_ATTACK_POINT: u32 = 6;
/// Tower attack projectile speed, world units per second.
pub const TOWER_PROJECTILE_SPEED: i32 = 750;
/// Tower armor.
pub const TOWER_ARMOR: i32 = 12;
/// Tower collision radius.
pub const TOWER_RADIUS: i32 = 40;
/// Tower fog light radius.
pub const TOWER_VISION: i32 = 1900;
/// Gold paid to the killer of a tower.
pub const TOWER_BOUNTY: i32 = 200;

/// Ancient health.
pub const ANCIENT_HP: i32 = 4500;
/// Ancient armor.
pub const ANCIENT_ARMOR: i32 = 13;
/// Ancient collision radius.
pub const ANCIENT_RADIUS: i32 = 72;
/// Ancient fog light radius.
pub const ANCIENT_VISION: i32 = 1200;

/// Fountain attack damage.
pub const FOUNTAIN_ATTACK_DAMAGE: i32 = 60;
/// Fountain attack range.
pub const FOUNTAIN_ATTACK_RANGE: i32 = 550;
/// Ticks between fountain attack starts.
pub const FOUNTAIN_ATTACK_INTERVAL: u32 = 6;
/// Ticks from fountain attack start to the hit.
pub const FOUNTAIN_ATTACK_POINT: u32 = 2;
/// Fountain collision radius.
pub const FOUNTAIN_RADIUS: i32 = 60;
/// Fountain fog light radius.
pub const FOUNTAIN_VISION: i32 = 1800;
/// Health restored per tick to allies inside the fountain area.
pub const FOUNTAIN_HEAL_HP_PER_TICK: i32 = 25;
/// Mana restored per tick to allies inside the fountain area.
pub const FOUNTAIN_HEAL_MANA_PER_TICK: i32 = 15;
/// Radius of the fountain heal area.
pub const FOUNTAIN_HEAL_RADIUS: i32 = 500;

// Combat.

/// How far past attack range a started attack still connects.
pub const ATTACK_RANGE_LEEWAY: i32 = 100;
/// How far an idle unit looks for something to attack.
pub const ACQUISITION_RANGE: i32 = 600;
/// Denominator scale of the armor formula: each point of armor adds
/// `ARMOR_SCALE` to a base of one hundred.
pub const ARMOR_SCALE: i32 = 6;

// Economy.

/// Gold each seat starts the match with.
pub const STARTING_GOLD: i32 = 600;
/// One gold arrives every this many ticks.
pub const PASSIVE_GOLD_PERIOD_TICKS: u32 = 30;
/// Gold for killing a hero, before the streak bonus.
pub const HERO_KILL_BOUNTY_BASE: i32 = 200;
/// Extra gold per kill in the victim's streak.
pub const HERO_KILL_BOUNTY_PER_STREAK: i32 = 50;
/// The streak bonus stops growing past this many kills.
pub const HERO_KILL_STREAK_CAP: i32 = 6;
/// Experience for killing a hero, before the per-level term.
pub const HERO_KILL_XP_BASE: i32 = 100;
/// Extra experience per level of the killed hero.
pub const HERO_KILL_XP_PER_LEVEL: i32 = 40;
/// Radius around a death within which enemy heroes receive experience.
pub const XP_RADIUS: i32 = 1500;

/// A friendly creep may be denied when its health is strictly below this
/// fraction of the maximum, expressed as a percent.
pub const DENY_HP_PCT: i32 = 50;
/// Denied creeps grant this percent of their experience.
pub const DENIED_XP_PCT: i32 = 50;

/// Helper for constants that are distances: a whole number of world units.
pub const fn units(n: i32) -> Fixed {
    Fixed::from_int(n)
}
