//! Working out what each entity fights by, from scratch, every tick.

use bota_proto::Fixed;

use crate::engine::{
    Def, EntityAllocator, Growth, Health, Inventory, Level, Mana, Stats, StatusKind, Statuses,
    Table, UnitDef, Upgrades,
};

/// What working out stats reads and writes.
///
/// The set is wide enough that naming the tables one by one runs past what is
/// readable at a call site; gathered here, the access a system takes is still
/// declared, and still checked when [`World::step`] hands the tables over.
///
/// [`World::step`]: crate::engine::World::step
pub struct StatsCx<'a> {
    /// Which entities exist.
    pub entities: &'a EntityAllocator,
    /// Which kind of unit each entity is.
    pub def: &'a Table<Def>,
    /// Hero levels.
    pub level: &'a Table<Level>,
    /// Upgrade intervals a creep spawned after.
    pub upgrades: &'a Table<Upgrades>,
    /// What each entity carries.
    pub inventory: &'a Table<Inventory>,
    /// What is on each entity.
    pub statuses: &'a Table<Statuses>,
    /// Where the answer goes.
    pub stats: &'a mut Table<Stats>,
    /// Health, which follows its maximum.
    pub health: &'a mut Table<Health>,
    /// Mana, which follows its maximum.
    pub mana: &'a mut Table<Mana>,
}

/// Rewrites every entity's [`Stats`] from the kind of unit it is, how far it
/// has been raised, and what is on it.
///
/// A pool follows its maximum: gaining maximum health gains the same health,
/// and losing it never leaves a pool above the new maximum.
pub fn derive_stats(cx: StatsCx<'_>) {
    let StatsCx {
        entities,
        def,
        level,
        upgrades,
        inventory,
        statuses,
        stats,
        health,
        mana,
    } = cx;
    for entity in entities.iter() {
        let Some(Def(kind)) = def.get(entity) else {
            continue;
        };
        let levels = level.get(entity).map_or(0, |l| i32::from(l.0.max(1) - 1));
        let steps = upgrades.get(entity).map_or(0, |u| u.0 as i32);
        let mut now = raised(kind, levels, steps);
        if let Some(bag) = inventory.get(entity) {
            let carried = crate::engine::carried_bonus(bag);
            now.max_hp += Fixed::from_int(carried.hp);
            now.max_mana += Fixed::from_int(carried.mana);
            now.damage += carried.damage;
            now.armor += carried.armor;
            now.move_speed += Fixed::from_int(carried.move_speed);
        }
        if let Some(on_it) = statuses.get(entity) {
            for status in on_it.active() {
                match status.kind {
                    StatusKind::Haste => {
                        let kept = (100 - status.magnitude).clamp(1, 100) as u32;
                        now.attack_interval = now.attack_interval * kept / 100;
                    }
                    StatusKind::Mending => {
                        now.hp_regen += Fixed::from_ratio(status.magnitude, 100);
                    }
                    StatusKind::Clarity => {
                        now.mana_regen += Fixed::from_ratio(status.magnitude, 100);
                    }
                }
            }
        }
        let before = stats.get(entity).copied();
        if let Some(hp) = health.get_mut(entity) {
            hp.hp = follow(hp.hp, before.map(|b| b.max_hp), now.max_hp);
        }
        if let Some(mp) = mana.get_mut(entity) {
            mp.mana = follow(mp.mana, before.map(|b| b.max_mana), now.max_mana);
        }
        stats.insert(entity, now);
    }
}

/// The plain form of a kind raised by `levels` levels and `steps` upgrades.
fn raised(kind: &UnitDef, levels: i32, steps: i32) -> Stats {
    let gained = |g: &Growth, per: &Growth| Growth {
        hp: g.hp * levels + per.hp * steps,
        mana: g.mana * levels + per.mana * steps,
        damage: g.damage * levels + per.damage * steps,
        armor_halves: g.armor_halves * levels + per.armor_halves * steps,
        gold: g.gold * levels + per.gold * steps,
        xp: g.xp * levels + per.xp * steps,
    };
    let up = gained(&kind.per_level, &kind.per_upgrade);
    Stats {
        max_hp: Fixed::from_int(kind.max_hp + up.hp),
        max_mana: Fixed::from_int(kind.max_mana + up.mana),
        hp_regen: kind.hp_regen,
        mana_regen: kind.mana_regen,
        damage: kind.damage + up.damage,
        attack_range: Fixed::from_int(kind.attack_range),
        acquisition: Fixed::from_int(kind.acquisition),
        attack_interval: kind.attack_interval,
        attack_point: kind.attack_point,
        attack_backswing: kind.attack_backswing,
        projectile_speed: kind.projectile_speed.map(Fixed::from_int),
        armor: kind.armor + up.armor_halves / 2,
        magic_resist_pct: kind.magic_resist_pct,
        move_speed: Fixed::from_int(kind.move_speed),
        turn_rate: kind.turn_rate,
        vision: Fixed::from_int(kind.vision),
        invulnerable: kind.invulnerable,
    }
}

/// What a pool holds once its maximum has moved.
///
/// With no maximum behind it yet the pool is only capped, never filled: what
/// an entity spawns with is whatever put it there.
fn follow(held: Fixed, was: Option<Fixed>, now: Fixed) -> Fixed {
    let Some(was) = was else {
        return held.min(now);
    };
    let grown = if now > was { held + (now - was) } else { held };
    grown.min(now)
}
