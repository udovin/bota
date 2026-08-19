//! Picking a target, swinging at it, and what a landed hit does.

use bota_proto::{DamageKind, EventKind, Fixed, Team, UnitKind};

use crate::engine::{Entity, Projectile, Windup, World, is_structure, wire_id};
use crate::sim::{Event, EventVisibility, rules};

/// A hit waiting to be applied.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    /// Who dealt it, while that entity still stands.
    pub source: Option<Entity>,
    /// Who takes it.
    pub target: Entity,
    /// Before armor and resistance.
    pub amount: i32,
    /// Which reduction applies.
    pub kind: DamageKind,
}

impl World {
    /// Each entity that can attack takes the best hostile in reach of its
    /// acquisition, and keeps it while it lives and stays in range.
    ///
    /// A lane creep is left out: it has a mind of its own, which runs first.
    pub fn acquire_targets(&mut self) {
        for entity in self.entities.iter().collect::<Vec<_>>() {
            if self.attacking.get(entity).is_none() || self.lane_ai.get(entity).is_some() {
                continue;
            }
            if let Some(held) = self.engage.get(entity).copied()
                && self.alive(held)
                && self.within_acquisition(entity, held)
            {
                continue;
            }
            let range = self
                .stats
                .get(entity)
                .map_or(Fixed::ZERO, |s| s.acquisition);
            let order = self.priority_of(entity);
            match self.acquire(entity, range, order) {
                Some(found) => {
                    self.engage.insert(entity, found);
                }
                None => {
                    self.engage.remove(entity);
                }
            }
        }
    }

    /// Whether an entity is still standing.
    pub fn alive(&self, entity: Entity) -> bool {
        self.entities.contains(entity)
            && self.health.get(entity).is_some_and(|h| h.hp > Fixed::ZERO)
    }

    /// Whether a held target is still inside acquisition range.
    fn within_acquisition(&self, seeker: Entity, target: Entity) -> bool {
        let (Some(at), Some(their_at), Some(stats)) = (
            self.transform.get(seeker),
            self.transform.get(target),
            self.stats.get(seeker),
        ) else {
            return false;
        };
        at.pos.within(their_at.pos, stats.acquisition)
    }

    /// Whether an attacker reaches its target, edge to edge.
    pub fn in_reach(&self, attacker: Entity, target: Entity) -> bool {
        let (Some(at), Some(their_at), Some(stats)) = (
            self.transform.get(attacker),
            self.transform.get(target),
            self.stats.get(attacker),
        ) else {
            return false;
        };
        let hulls = self.hull.get(attacker).map_or(Fixed::ZERO, |h| h.radius)
            + self.hull.get(target).map_or(Fixed::ZERO, |h| h.radius);
        at.pos.within(their_at.pos, stats.attack_range + hulls)
    }

    /// Runs down every swing, lands what has come due, and starts new ones.
    pub fn swing(&mut self, hits: &mut Vec<Hit>) {
        for entity in self.entities.iter().collect::<Vec<_>>() {
            let Some(state) = self.attacking.get(entity).copied() else {
                continue;
            };
            let mut state = state;
            state.cooldown = state.cooldown.saturating_sub(1);
            state.recovering = state.recovering.saturating_sub(1);
            match state.windup {
                Some(windup) if windup.ticks_left > 1 => {
                    state.windup = Some(Windup {
                        target: windup.target,
                        ticks_left: windup.ticks_left - 1,
                    });
                }
                Some(windup) => {
                    state.windup = None;
                    state.recovering = self.stats.get(entity).map_or(0, |s| s.attack_backswing);
                    self.land(entity, windup.target, hits);
                }
                None => {
                    if state.cooldown == 0
                        && let Some(target) = self.engage.get(entity).copied()
                        && self.alive(target)
                        && self.in_reach(entity, target)
                        && let Some(stats) = self.stats.get(entity)
                        && stats.damage > 0
                    {
                        state.windup = Some(Windup {
                            target,
                            ticks_left: stats.attack_point.max(1),
                        });
                        state.cooldown = stats.attack_interval;
                    }
                }
            }
            self.attacking.insert(entity, state);
        }
    }

    /// A swing that has come due: a hit at once, or a missile on its way.
    fn land(&mut self, attacker: Entity, target: Entity, hits: &mut Vec<Hit>) {
        let Some(stats) = self.stats.get(attacker).copied() else {
            return;
        };
        if !self.alive(target) {
            return;
        }
        match stats.projectile_speed {
            None => {
                if self.in_reach(attacker, target) {
                    hits.push(Hit {
                        source: Some(attacker),
                        target,
                        amount: stats.damage,
                        kind: DamageKind::Physical,
                    });
                }
            }
            Some(speed) => {
                let Some(at) = self.transform.get(attacker).copied() else {
                    return;
                };
                let side = self.team.get(attacker).copied().unwrap_or(Team::Neutral);
                let missile = self.spawn();
                self.transform.insert(missile, at);
                self.set_team(missile, side);
                self.projectile.insert(
                    missile,
                    Projectile {
                        speed,
                        source: Some(attacker),
                        target,
                        damage: stats.damage,
                        kind: DamageKind::Physical,
                        ability: None,
                        launch_tier: 0,
                        crit: false,
                        bounces_left: 0,
                        bounced: Vec::new(),
                    },
                );
            }
        }
    }

    /// Moves every missile and lands the ones that have arrived.
    pub fn fly(&mut self, hits: &mut Vec<Hit>) {
        for missile in self.entities.iter().collect::<Vec<_>>() {
            let Some(shot) = self.projectile.get(missile).cloned() else {
                continue;
            };
            if !self.alive(shot.target) {
                self.despawn(missile);
                continue;
            }
            let Some(to) = self.transform.get(shot.target).map(|t| t.pos) else {
                self.despawn(missile);
                continue;
            };
            let Some(at) = self.transform.get_mut(missile) else {
                continue;
            };
            let step = crate::sim::per_tick(shot.speed);
            let next = crate::sim::move_towards(at.pos, to, step);
            at.pos = next;
            at.facing = crate::sim::facing_towards(next, to);
            if next == to {
                hits.push(Hit {
                    source: shot.source,
                    target: shot.target,
                    amount: shot.damage,
                    kind: shot.kind,
                });
                if !self.bounce_on(missile, shot.target) {
                    self.despawn(missile);
                }
            }
        }
    }

    /// Takes every hit off the health it lands on, and tells who may know.
    ///
    /// What falls is reported with whoever struck it last.
    pub fn resolve(
        &mut self,
        hits: Vec<Hit>,
        events: &mut Vec<Event>,
    ) -> Vec<(Entity, Option<Entity>)> {
        let mut fallen = Vec::new();
        for hit in hits {
            if !self.alive(hit.target) {
                continue;
            }
            let Some(stats) = self.stats.get(hit.target).copied() else {
                continue;
            };
            if stats.invulnerable {
                continue;
            }
            let taken = mitigate(hit.amount, hit.kind, stats.armor, stats.magic_resist_pct);
            let Some(health) = self.health.get_mut(hit.target) else {
                continue;
            };
            let applied = taken.min(health.hp.to_int().max(0) + 1);
            health.hp -= Fixed::from_int(applied);
            let down = health.hp <= Fixed::ZERO;
            let at = self
                .transform
                .get(hit.target)
                .map_or(bota_proto::Vec2::ZERO, |t| t.pos);
            let involved = self.team.get(hit.target).copied().unwrap_or(Team::Neutral);
            events.push(Event {
                kind: EventKind::Damaged {
                    source: hit.source.map(wire_id),
                    target: wire_id(hit.target),
                    amount: applied,
                    kind: hit.kind,
                    crit: false,
                },
                visible_to: self.who_may_know(at, involved),
            });
            if down {
                fallen.push((hit.target, hit.source));
            }
        }
        fallen
    }

    /// Clears away what has fallen and tells who may know.
    pub fn bury(&mut self, fallen: Vec<(Entity, Option<Entity>)>, events: &mut Vec<Event>) {
        for (entity, killer) in fallen {
            if !self.entities.contains(entity) {
                continue;
            }
            let kind = self.kind.get(entity).copied();
            let side = self.team.get(entity).copied();
            let denied = killer
                .and_then(|k| self.team.get(k).copied())
                .is_some_and(|theirs| Some(theirs) == side);
            self.pay_for(entity, killer, events);
            let at = self
                .transform
                .get(entity)
                .map_or(bota_proto::Vec2::ZERO, |t| t.pos);
            events.push(Event {
                kind: EventKind::Died {
                    unit: wire_id(entity),
                    killer: killer.map(wire_id),
                    denied,
                },
                visible_to: self.who_may_know(at, side.unwrap_or(Team::Neutral)),
            });
            if let (Some(kind), Some(side)) = (kind, side)
                && is_structure(kind)
            {
                events.push(Event {
                    kind: EventKind::StructureDestroyed {
                        unit: wire_id(entity),
                        team: side,
                    },
                    visible_to: EventVisibility::Everyone,
                });
                if kind == UnitKind::Ancient {
                    self.winner = Some(other_side(side));
                }
            }
            for index in 0..self.seats.len() {
                if self.seats[index].unit == Some(entity) {
                    let level = self.seats[index].level;
                    self.seats[index].unit = None;
                    self.seats[index].deaths += 1;
                    self.seats[index].respawn_left = World::respawn_wait(level);
                }
            }
            if let Some(index) =
                killer.and_then(|k| self.seats.iter().position(|s| s.unit == Some(k)))
                && kind == Some(UnitKind::Hero)
                && !denied
            {
                self.seats[index].kills += 1;
            }
            self.despawn(entity);
        }
    }
}

/// The side that is not this one.
fn other_side(team: Team) -> Team {
    match team {
        Team::Radiant => Team::Dire,
        Team::Dire => Team::Radiant,
        Team::Neutral => Team::Neutral,
    }
}

/// Damage after armor or magic resistance.
fn mitigate(amount: i32, kind: DamageKind, armor: i32, magic_resist_pct: i32) -> i32 {
    match kind {
        DamageKind::Physical => {
            let den = 100 + rules::ARMOR_SCALE * armor.max(0);
            (i64::from(amount) * 100 / i64::from(den)) as i32
        }
        DamageKind::Magical => {
            let kept = (100 - magic_resist_pct).clamp(0, 100);
            (i64::from(amount) * i64::from(kept) / 100) as i32
        }
        DamageKind::Pure => amount,
    }
}
