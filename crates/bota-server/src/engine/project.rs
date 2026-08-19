//! The world as a side is allowed to see it.

use bota_proto::{Fixed, PlayerView, ProjectileView, StatusFlags, Team, UnitView, WorldView};

use crate::engine::{Entity, World};

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
                .filter(|entity| self.projectile.get(*entity).is_some())
                .filter(|entity| match viewer {
                    None => true,
                    Some(team) => {
                        self.team.get(*entity) == Some(&team)
                            || self.visibility.get(*entity).is_some_and(|s| s.by(team))
                    }
                })
                .filter_map(|entity| {
                    let shot = self.projectile.get(entity)?;
                    let at = self.transform.get(entity)?;
                    Some(ProjectileView {
                        id: wire_id(entity),
                        pos: at.pos,
                        facing: at.facing,
                        team: self.team.get(entity).copied().unwrap_or(Team::Neutral),
                        ability: shot.ability,
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
                        _ => Some(vec![None; seat.stash.slots.len()]),
                    },
                    kills: seat.kills,
                    deaths: seat.deaths,
                    assists: seat.assists,
                    last_hits: seat.last_hits,
                    denies: seat.denies,
                    respawn_left: seat.respawn_left,
                })
                .collect(),
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
            statuses: StatusFlags::default(),
            hero: self.hero.get(entity).copied(),
            owner: self.owner.get(entity).copied(),
            level: self.level.get(entity).map_or(0, |l| l.0),
            abilities: Vec::new(),
            items: Vec::new(),
            effects: Vec::new(),
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
