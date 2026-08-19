//! The numbers an entity fights by, worked out afresh every tick.

use bota_proto::Fixed;

/// Everything the type an entity is, its level, its items and what is on it
/// add up to.
///
/// Written by the system that works stats out and read by everything else.
/// Nothing else writes here: a value put in by hand is gone next tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
    /// The most health it can hold.
    pub max_hp: Fixed,
    /// The most mana it can hold.
    pub max_mana: Fixed,
    /// Health mended each tick.
    pub hp_regen: Fixed,
    /// Mana mended each tick.
    pub mana_regen: Fixed,
    /// Damage one attack deals.
    pub damage: i32,
    /// How far it reaches, edge to edge.
    pub attack_range: Fixed,
    /// How far it looks for something to attack.
    pub acquisition: Fixed,
    /// Ticks between the starts of two attacks.
    pub attack_interval: u32,
    /// Ticks from the start of an attack to the hit.
    pub attack_point: u32,
    /// Ticks after the hit before it may move again.
    pub attack_backswing: u32,
    /// Speed of the missile it throws. Absent for a melee attack.
    pub projectile_speed: Option<Fixed>,
    /// Armor, reducing physical damage.
    pub armor: i32,
    /// Magic resistance, percent.
    pub magic_resist_pct: i32,
    /// World units per second on the ground.
    pub move_speed: Fixed,
    /// Brads per tick it turns.
    pub turn_rate: u16,
    /// How far it sees.
    pub vision: Fixed,
    /// Whether damage passes it by.
    pub invulnerable: bool,
}
