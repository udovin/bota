//! The things that stand on the map, and the per-seat state behind heroes.

use bota_proto::{Angle, EntityId, Fixed, HeroId, SlotId, Team, UnitKind, Vec2};

use crate::sim::rules;

/// What a unit is currently trying to do.
///
/// For a hero this is the standing player order; for creeps and buildings it is
/// written by the aggro pass. The tactical attack target lives in
/// [`Unit::engage`], not here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitOrder {
    /// Stand still. Acquires nearby enemies on its own.
    Idle,
    /// Stand still, attack what comes into range, never move.
    Hold,
    /// Walk to a position, ignoring enemies.
    Move {
        /// Destination.
        pos: Vec2,
    },
    /// Walk to a position, engaging enemies encountered on the way.
    AttackMove {
        /// Destination.
        pos: Vec2,
    },
    /// Attack one unit, chasing it while it stays visible.
    Attack {
        /// The target.
        target: EntityId,
        /// Where the target was last seen by this unit's team.
        last_seen: Vec2,
    },
}

/// An attack that has started but not yet connected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Windup {
    /// Who the attack is aimed at.
    pub target: EntityId,
    /// Ticks until the hit lands or the projectile leaves.
    pub ticks_left: u32,
}

/// One unit on the map.
///
/// A single struct covers heroes, creeps and buildings; `kind` and the optional
/// fields tell them apart. Dead units are removed from the arena rather than
/// flagged.
#[derive(Clone, Debug)]
pub struct Unit {
    /// What kind of unit this is.
    pub kind: UnitKind,
    /// Which side it belongs to.
    pub team: Team,
    /// Current position.
    pub pos: Vec2,
    /// Which way it is facing.
    pub facing: Angle,
    /// Current health.
    pub hp: i32,
    /// Maximum health.
    pub max_hp: i32,
    /// Current mana.
    pub mana: i32,
    /// Maximum mana.
    pub max_mana: i32,
    /// Movement speed in world units per second. Zero for buildings.
    pub move_speed: Fixed,
    /// Attack damage per hit, before the target's armor.
    pub attack_damage: i32,
    /// Attack range. Zero for units that cannot attack.
    pub attack_range: Fixed,
    /// Ticks between attack starts.
    pub attack_interval: u32,
    /// Ticks from attack start to the hit or the projectile leaving.
    pub attack_point: u32,
    /// Speed of the attack projectile. Absent for instant hits.
    pub projectile_speed: Option<Fixed>,
    /// Armor, in whole points.
    pub armor: i32,
    /// Magic resistance, percent.
    pub magic_resist_pct: i32,
    /// Radius the unit occupies.
    pub radius: Fixed,
    /// How far it lights the fog for its team. Zero lights nothing.
    pub vision_radius: Fixed,
    /// Cannot be targeted or damaged.
    pub invulnerable: bool,
    
    /// The standing order.
    pub order: UnitOrder,
    /// Current attack target, chosen from the order or by aggro.
    pub engage: Option<EntityId>,
    /// The attack in progress.
    pub windup: Option<Windup>,
    /// Ticks until the next attack may start.
    pub attack_cooldown: u32,
    /// Which seat controls it, for heroes.
    pub owner: Option<SlotId>,
    /// Which hero it is, for heroes.
    pub hero: Option<HeroId>,
    /// Hero level. Zero for everything else.
    pub level: u8,
    /// Gold for killing it. Zero where a kill pays nothing.
    pub bounty: i32,
    /// Experience granted to nearby enemy heroes on death.
    pub xp_reward: i32,
}

impl Unit {
    /// Whether this unit can attack at all.
    pub fn can_attack(&self) -> bool {
        self.attack_range > Fixed::ZERO && self.attack_damage > 0
    }

    /// Whether this unit is a building.
    pub fn is_structure(&self) -> bool {
        matches!(
            self.kind,
            UnitKind::Tower | UnitKind::Ancient | UnitKind::Fountain
        )
    }

    /// Whether this unit is a lane creep.
    pub fn is_creep(&self) -> bool {
        matches!(
            self.kind,
            UnitKind::CreepMelee | UnitKind::CreepRanged | UnitKind::CreepSiege
        )
    }

    /// A generic hero for a seat.
    pub fn hero(team: Team, owner: SlotId, hero: HeroId, level: u8, pos: Vec2) -> Unit {
        let above = i32::from(level.saturating_sub(1));
        Unit {
            kind: UnitKind::Hero,
            team,
            pos,
            facing: Angle::default(),
            hp: rules::HERO_HP + rules::HERO_HP_PER_LEVEL * above,
            max_hp: rules::HERO_HP + rules::HERO_HP_PER_LEVEL * above,
            mana: rules::HERO_MANA + rules::HERO_MANA_PER_LEVEL * above,
            max_mana: rules::HERO_MANA + rules::HERO_MANA_PER_LEVEL * above,
            move_speed: rules::units(rules::HERO_MOVE_SPEED),
            attack_damage: rules::HERO_ATTACK_DAMAGE + rules::HERO_ATTACK_DAMAGE_PER_LEVEL * above,
            attack_range: rules::units(rules::HERO_ATTACK_RANGE),
            attack_interval: rules::HERO_ATTACK_INTERVAL,
            attack_point: rules::HERO_ATTACK_POINT,
            projectile_speed: Some(rules::units(rules::HERO_PROJECTILE_SPEED)),
            armor: rules::HERO_ARMOR,
            magic_resist_pct: rules::HERO_MAGIC_RESIST_PCT,
            radius: rules::units(rules::HERO_RADIUS),
            vision_radius: rules::units(rules::HERO_VISION),
            invulnerable: false,
            order: UnitOrder::Idle,
            engage: None,
            windup: None,
            attack_cooldown: 0,
            owner: Some(owner),
            hero: Some(hero),
            level,
            bounty: 0,
            xp_reward: 0,
        }
    }

    /// A melee lane creep.
    pub fn melee_creep(team: Team, pos: Vec2) -> Unit {
        Unit {
            kind: UnitKind::CreepMelee,
            team,
            pos,
            facing: Angle::default(),
            hp: rules::MELEE_CREEP_HP,
            max_hp: rules::MELEE_CREEP_HP,
            mana: 0,
            max_mana: 0,
            move_speed: rules::units(rules::CREEP_MOVE_SPEED),
            attack_damage: rules::MELEE_CREEP_ATTACK_DAMAGE,
            attack_range: rules::units(rules::MELEE_CREEP_ATTACK_RANGE),
            attack_interval: rules::CREEP_ATTACK_INTERVAL,
            attack_point: rules::CREEP_ATTACK_POINT,
            projectile_speed: None,
            armor: rules::MELEE_CREEP_ARMOR,
            magic_resist_pct: 0,
            radius: rules::units(rules::CREEP_RADIUS),
            vision_radius: rules::units(rules::CREEP_VISION),
            invulnerable: false,
            order: UnitOrder::Idle,
            engage: None,
            windup: None,
            attack_cooldown: 0,
            owner: None,
            hero: None,
            level: 0,
            bounty: rules::MELEE_CREEP_BOUNTY,
            xp_reward: rules::MELEE_CREEP_XP,
        }
    }

    /// A ranged lane creep.
    pub fn ranged_creep(team: Team, pos: Vec2) -> Unit {
        Unit {
            kind: UnitKind::CreepRanged,
            hp: rules::RANGED_CREEP_HP,
            max_hp: rules::RANGED_CREEP_HP,
            attack_damage: rules::RANGED_CREEP_ATTACK_DAMAGE,
            attack_range: rules::units(rules::RANGED_CREEP_ATTACK_RANGE),
            projectile_speed: Some(rules::units(rules::CREEP_PROJECTILE_SPEED)),
            armor: 0,
            bounty: rules::RANGED_CREEP_BOUNTY,
            xp_reward: rules::RANGED_CREEP_XP,
            ..Unit::melee_creep(team, pos)
        }
    }

    /// A siege creep.
    pub fn siege_creep(team: Team, pos: Vec2) -> Unit {
        Unit {
            kind: UnitKind::CreepSiege,
            hp: rules::SIEGE_CREEP_HP,
            max_hp: rules::SIEGE_CREEP_HP,
            attack_damage: rules::SIEGE_CREEP_ATTACK_DAMAGE,
            attack_range: rules::units(rules::SIEGE_CREEP_ATTACK_RANGE),
            attack_interval: rules::SIEGE_CREEP_ATTACK_INTERVAL,
            projectile_speed: Some(rules::units(rules::CREEP_PROJECTILE_SPEED)),
            armor: rules::SIEGE_CREEP_ARMOR,
            radius: rules::units(rules::CREEP_RADIUS + 8),
            bounty: rules::SIEGE_CREEP_BOUNTY,
            xp_reward: rules::SIEGE_CREEP_XP,
            ..Unit::melee_creep(team, pos)
        }
    }

    /// A lane tower.
    pub fn tower(team: Team, pos: Vec2) -> Unit {
        Unit {
            kind: UnitKind::Tower,
            team,
            pos,
            facing: Angle::default(),
            hp: rules::TOWER_HP,
            max_hp: rules::TOWER_HP,
            mana: 0,
            max_mana: 0,
            move_speed: Fixed::ZERO,
            attack_damage: rules::TOWER_ATTACK_DAMAGE,
            attack_range: rules::units(rules::TOWER_ATTACK_RANGE),
            attack_interval: rules::TOWER_ATTACK_INTERVAL,
            attack_point: rules::TOWER_ATTACK_POINT,
            projectile_speed: Some(rules::units(rules::TOWER_PROJECTILE_SPEED)),
            armor: rules::TOWER_ARMOR,
            magic_resist_pct: 0,
            radius: rules::units(rules::TOWER_RADIUS),
            vision_radius: rules::units(rules::TOWER_VISION),
            invulnerable: false,
            order: UnitOrder::Hold,
            engage: None,
            windup: None,
            attack_cooldown: 0,
            owner: None,
            hero: None,
            level: 0,
            bounty: rules::TOWER_BOUNTY,
            xp_reward: 0,
        }
    }

    /// The Ancient.
    pub fn ancient(team: Team, pos: Vec2) -> Unit {
        Unit {
            kind: UnitKind::Ancient,
            hp: rules::ANCIENT_HP,
            max_hp: rules::ANCIENT_HP,
            attack_damage: 0,
            attack_range: Fixed::ZERO,
            projectile_speed: None,
            armor: rules::ANCIENT_ARMOR,
            radius: rules::units(rules::ANCIENT_RADIUS),
            vision_radius: rules::units(rules::ANCIENT_VISION),
            bounty: 0,
            ..Unit::tower(team, pos)
        }
    }

    /// The fountain.
    pub fn fountain(team: Team, pos: Vec2) -> Unit {
        Unit {
            kind: UnitKind::Fountain,
            hp: 1,
            max_hp: 1,
            attack_damage: rules::FOUNTAIN_ATTACK_DAMAGE,
            attack_range: rules::units(rules::FOUNTAIN_ATTACK_RANGE),
            attack_interval: rules::FOUNTAIN_ATTACK_INTERVAL,
            attack_point: rules::FOUNTAIN_ATTACK_POINT,
            projectile_speed: None,
            armor: 0,
            radius: rules::units(rules::FOUNTAIN_RADIUS),
            vision_radius: rules::units(rules::FOUNTAIN_VISION),
            invulnerable: true,
            bounty: 0,
            ..Unit::tower(team, pos)
        }
    }
}

/// Per-seat state that survives the death of the hero unit.
#[derive(Clone, Debug)]
pub struct SeatState {
    /// Which seat this is.
    pub slot: SlotId,
    /// Which side it plays for.
    pub team: Team,
    /// Which hero it picked.
    pub hero: HeroId,
    /// The hero's unit. Absent while dead.
    pub unit: Option<EntityId>,
    /// Unspent gold.
    pub gold: i32,
    /// Gold earned over the whole match, spent or not.
    pub net_worth: i32,
    /// Total experience.
    pub xp: i32,
    /// Hero level.
    pub level: u8,
    /// Kills scored.
    pub kills: u16,
    /// Times died.
    pub deaths: u16,
    /// Kills assisted.
    pub assists: u16,
    /// Enemy creeps last hit.
    pub last_hits: u16,
    /// Friendly creeps denied.
    pub denies: u16,
    /// Damage dealt to enemy heroes.
    pub hero_damage: i32,
    /// Damage dealt to enemy buildings.
    pub structure_damage: i32,
    /// Ticks until respawn. Zero while alive.
    pub respawn_left: u32,
    /// Kills since last dying. Feeds the bounty streak bonus.
    pub kill_streak: i32,
}
