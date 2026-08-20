//! The world as a side is allowed to see it.

use bota_proto::{
    AbilityView, EffectId, EffectView, Fixed, PlayerView, ProjectileView, StatusFlags, Team,
    UnitView, WorldView,
};

use crate::game::{Entity, StatusKind, World, ability_mana_cost, item_views};

/// The handle as it travels on the wire.
pub fn wire_id(entity: Entity) -> bota_proto::EntityId {
    bota_proto::EntityId {
        idx: entity.index().0,
        generation: entity.generation().0.get(),
    }
}

impl World {
    /// The world through one side's fog of war.
    pub fn view(&self, team: Team) -> WorldView {
        self.project(Some(team))
    }

    /// The whole world, for spectators and the replay.
    pub fn view_full(&self) -> WorldView {
        self.project(None)
    }

    /// Everything a viewer is allowed to be told. `None` holds nothing back.
    fn project(&self, viewer: Option<Team>) -> WorldView {
        let units = self
            .entities
            .iter()
            .filter(|entity| match viewer {
                None => true,
                Some(team) => {
                    self.team.get(*entity) == Some(&team)
                        || self.visibility.get(*entity).is_some_and(|s| s.by(team))
                }
            })
            .filter_map(|entity| self.project_unit(entity))
            .collect();
        WorldView {
            tick: self.tick,
            viewer,
            units,
            projectiles: self
                .entities
                .iter()
                .filter(|entity| {
                    self.projectile.get(*entity).is_some() || self.hook.get(*entity).is_some()
                })
                .filter(|entity| match viewer {
                    None => true,
                    Some(team) => {
                        self.team.get(*entity) == Some(&team)
                            || self.visibility.get(*entity).is_some_and(|s| s.by(team))
                    }
                })
                .filter_map(|entity| {
                    let at = self.transform.get(entity)?;
                    let ability = match self.projectile.get(entity) {
                        Some(shot) => shot.ability,
                        None => Some(crate::game::ability::MEAT_HOOK),
                    };
                    Some(ProjectileView {
                        id: wire_id(entity),
                        pos: at.pos,
                        facing: at.facing,
                        team: self.team.get(entity).copied().unwrap_or(Team::Neutral),
                        ability,
                    })
                })
                .collect(),
            players: self
                .seats
                .iter()
                .map(|seat| PlayerView {
                    slot: seat.slot,
                    team: seat.team,
                    hero: seat.hero,
                    unit: seat.unit.map(wire_id),
                    level: seat.level,
                    xp: seat.xp,
                    // A side is told its own gold and its own stash, nobody
                    // else's.
                    gold: match viewer {
                        None => Some(seat.gold),
                        Some(team) if team == seat.team => Some(seat.gold),
                        Some(_) => None,
                    },
                    stash: match viewer {
                        Some(team) if team != seat.team => None,
                        _ => Some(item_views(&seat.stash)),
                    },
                    kills: seat.kills,
                    deaths: seat.deaths,
                    assists: seat.assists,
                    last_hits: seat.last_hits,
                    denies: seat.denies,
                    respawn_left: seat.respawn_left,
                })
                .collect(),
            felled_trees: self.trees.felled().collect(),
            planted_trees: self.trees.planted().iter().map(|tree| tree.at).collect(),
        }
    }

    /// One unit, or nothing when the entity is not one.
    fn project_unit(&self, entity: Entity) -> Option<UnitView> {
        let kind = *self.kind.get(entity)?;
        let transform = self.transform.get(entity)?;
        let stats = self.stats.get(entity)?;
        let health = self.health.get(entity);
        let mana = self.mana.get(entity);
        Some(UnitView {
            id: wire_id(entity),
            kind,
            team: self.team.get(entity).copied().unwrap_or(Team::Neutral),
            pos: transform.pos,
            facing: transform.facing,
            hp: shown(health.map_or(Fixed::ZERO, |h| h.hp)),
            max_hp: stats.max_hp.to_int(),
            mana: shown(mana.map_or(Fixed::ZERO, |m| m.mana)),
            max_mana: stats.max_mana.to_int(),
            move_speed: stats.move_speed,
            attack_damage: stats.damage,
            attack_range: stats.attack_range,
            attack_interval: stats.attack_interval,
            armor: Fixed::from_int(stats.armor),
            magic_resist: Fixed::from_ratio(stats.magic_resist_pct, 100),
            radius: self.hull.get(entity).map_or(Fixed::ZERO, |h| h.radius),
            vision_radius: stats.vision,
            true_sight_radius: stats.true_sight,
            statuses: StatusFlags {
                bits: self.state_of(entity)
                    | if stats.hides {
                        StatusFlags::INVISIBLE
                    } else {
                        0
                    },
            },
            hero: self.hero.get(entity).copied(),
            owner: self.owner.get(entity).copied(),
            level: self.level.get(entity).map_or(0, |l| l.0),
            abilities: self.abilities.get(entity).map_or_else(Vec::new, |book| {
                book.slots
                    .iter()
                    .map(|ability| AbilityView {
                        id: ability.id,
                        level: ability.level,
                        cooldown_left: ability.cooldown,
                        mana_cost: ability_mana_cost(ability.id, ability.level),
                    })
                    .collect()
            }),
            items: self.inventory.get(entity).map_or_else(Vec::new, item_views),
            effects: self.statuses.get(entity).map_or_else(Vec::new, |on_it| {
                on_it
                    .active()
                    .map(|status| EffectView {
                        id: EffectId(effect_id(status.kind)),
                        ticks_left: status.ticks_left,
                    })
                    .collect()
            }),
        })
    }
}

/// A pool as a number to show.
///
/// Anything left of a pool counts as one point, so a unit still standing never
/// reads as empty.
fn shown(held: Fixed) -> i32 {
    if held > Fixed::ZERO {
        held.to_int().max(1)
    } else {
        held.to_int()
    }
}

/// Which effect one kind is on the wire.
fn effect_id(kind: StatusKind) -> u16 {
    match kind {
        StatusKind::Haste { .. } => 0,
        StatusKind::Mending { .. } => 1,
        StatusKind::Clarity { .. } => 2,
        StatusKind::Fountain { .. } => 3,
        StatusKind::Stunned => 4,
        StatusKind::Slowed { .. } => 5,
        StatusKind::Burning { .. } => 6,
    }
}

impl World {
    /// The state a unit is in, as the wire names it.
    fn state_of(&self, entity: Entity) -> u16 {
        let Some(on_it) = self.statuses.get(entity) else {
            return 0;
        };
        let mut bits = 0;
        for status in on_it.active() {
            bits |= match status.kind {
                StatusKind::Stunned => StatusFlags::STUNNED,
                StatusKind::Slowed { .. } => StatusFlags::SLOWED,
                StatusKind::Burning { .. } => StatusFlags::DOT,
                _ => 0,
            };
        }
        bits
    }
}
