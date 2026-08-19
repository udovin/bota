//! Casting: what the four abilities do, and what stops a cast happening.

use bota_proto::{DamageKind, EventKind, Fixed, OrderTarget, Team};

use crate::engine::{Entity, Hit, PendingCast, Projectile, Status, StatusKind, World, wire_id};
use crate::sim::{Event, rules};

impl World {
    /// Whether an entity stands in its own shop.
    pub fn at_shop(&self, entity: Entity) -> bool {
        let (Some(at), Some(side)) = (
            self.transform.get(entity).map(|t| t.pos),
            self.team.get(entity).copied(),
        ) else {
            return false;
        };
        at.within(
            crate::sim::fountain_pos(self.map, side),
            rules::units(rules::SHOP_RANGE),
        )
    }

    /// Starts whatever cast each entity is waiting to make.
    ///
    /// A cast waits on nothing but its own cost: the level must be learned,
    /// the cooldown run out, and the mana be there. What it cannot pay for is
    /// dropped rather than held.
    pub fn run_casts(&mut self, events: &mut Vec<Event>, hits: &mut Vec<Hit>) {
        for entity in self.entities.iter().collect::<Vec<_>>() {
            let Some(cast) = self.casting.remove(entity) else {
                continue;
            };
            let slot = usize::from(cast.slot.0);
            let Some(ability) = self
                .abilities
                .get(entity)
                .and_then(|book| book.slots.get(slot))
                .copied()
            else {
                continue;
            };
            if ability.level == 0 || ability.cooldown > 0 {
                continue;
            }
            let level = usize::from(ability.level - 1);
            let cost = match ability.id.0 {
                1 => rules::SYLLA_FRENZY_MANA[level],
                2 => rules::SYLLA_BOUNCE_MANA[level],
                3 => rules::SYLLA_MULTI_MANA[level.min(2)],
                _ => continue,
            };
            if self.mana.get(entity).map_or(0, |m| m.mana.to_int()) < cost {
                continue;
            }
            let cooldown = match ability.id.0 {
                1 => rules::SYLLA_FRENZY_COOLDOWN[level],
                2 => rules::SYLLA_BOUNCE_COOLDOWN[level],
                3 => rules::SYLLA_MULTI_COOLDOWN[level.min(2)],
                _ => continue,
            };
            let went = match ability.id.0 {
                1 => self.cast_frenzy(entity, level),
                2 => self.cast_bounce(entity, level, cast.target),
                3 => self.cast_multishot(entity, level.min(2), hits),
                _ => false,
            };
            if !went {
                continue;
            }
            if let Some(mana) = self.mana.get_mut(entity) {
                mana.mana -= Fixed::from_int(cost);
            }
            if let Some(book) = self.abilities.get_mut(entity)
                && let Some(ability) = book.slots.get_mut(slot)
            {
                ability.cooldown = cooldown;
            }
            let at = self
                .transform
                .get(entity)
                .map_or(bota_proto::Vec2::ZERO, |t| t.pos);
            let side = self.team.get(entity).copied().unwrap_or(Team::Neutral);
            events.push(Event {
                kind: EventKind::AbilityCast {
                    caster: wire_id(entity),
                    ability: ability.id,
                },
                visible_to: self.who_may_know(at, side),
            });
        }
    }

    /// Puts haste on the caster for a while.
    fn cast_frenzy(&mut self, caster: Entity, level: usize) -> bool {
        let mut on_it = self.statuses.remove(caster).unwrap_or_default();
        on_it.0.retain(|held| held.kind != StatusKind::Haste);
        on_it.0.push(Status {
            kind: StatusKind::Haste,
            ticks_left: rules::SYLLA_FRENZY_TICKS,
            magnitude: rules::SYLLA_FRENZY_HASTE_PCT[level],
        });
        self.statuses.insert(caster, on_it);
        true
    }

    /// Throws a missile that goes on to the next enemy after each hit.
    fn cast_bounce(&mut self, caster: Entity, level: usize, target: OrderTarget) -> bool {
        let OrderTarget::Unit { target } = target else {
            return false;
        };
        let Some(mark) = self.of_wire(target) else {
            return false;
        };
        if !self.hostile(caster, mark) {
            return false;
        }
        let range = rules::units(rules::SYLLA_BOUNCE_CAST_RANGE);
        if !self.reachable(caster, range, mark) {
            return false;
        }
        let (Some(at), Some(side)) = (
            self.transform.get(caster).copied(),
            self.team.get(caster).copied(),
        ) else {
            return false;
        };
        let missile = self.spawn();
        self.transform.insert(missile, at);
        self.set_team(missile, side);
        self.projectile.insert(
            missile,
            Projectile {
                speed: Fixed::from_int(rules::SYLLA_BOUNCE_SPEED),
                source: Some(caster),
                target: mark,
                damage: rules::SYLLA_BOUNCE_DAMAGE[level],
                kind: DamageKind::Magical,
                ability: Some(bota_proto::AbilityId(2)),
                launch_tier: 0,
                crit: false,
                bounces_left: rules::SYLLA_BOUNCE_COUNT[level],
                bounced: vec![mark],
            },
        );
        true
    }

    /// Strikes every enemy standing near the caster at once.
    fn cast_multishot(&mut self, caster: Entity, level: usize, hits: &mut Vec<Hit>) -> bool {
        let Some(at) = self.transform.get(caster).map(|t| t.pos) else {
            return false;
        };
        let damage = self.stats.get(caster).map_or(0, |s| s.damage)
            * rules::SYLLA_MULTI_DMG_PCT[level]
            / 100;
        let radius = rules::units(rules::SYLLA_MULTI_RADIUS);
        let struck: Vec<Entity> = self
            .entities
            .iter()
            .filter(|other| {
                self.hostile(caster, *other)
                    && self
                        .transform
                        .get(*other)
                        .is_some_and(|t| t.pos.within(at, radius))
            })
            .collect();
        for mark in struck {
            hits.push(Hit {
                source: Some(caster),
                target: mark,
                amount: damage,
                kind: DamageKind::Physical,
            });
        }
        true
    }

    /// Sends a missile on to the next enemy near where it landed.
    ///
    /// It never strikes the same one twice, and stops once its bounces are
    /// spent or there is nobody left to go to.
    pub fn bounce_on(&mut self, missile: Entity, from: Entity) -> bool {
        let Some(mut shot) = self.projectile.get(missile).cloned() else {
            return false;
        };
        if shot.bounces_left == 0 {
            return false;
        }
        let Some(at) = self.transform.get(from).map(|t| t.pos) else {
            return false;
        };
        let radius = rules::units(rules::SYLLA_BOUNCE_RANGE);
        let Some(source) = shot.source else {
            return false;
        };
        let next = self
            .entities
            .iter()
            .filter(|other| {
                !shot.bounced.contains(other)
                    && self.hostile(source, *other)
                    && self
                        .transform
                        .get(*other)
                        .is_some_and(|t| t.pos.within(at, radius))
            })
            .min_by_key(|other| {
                self.transform
                    .get(*other)
                    .map_or(i64::MAX, |t| t.pos.distance_squared(at))
            });
        let Some(next) = next else {
            return false;
        };
        shot.bounces_left -= 1;
        shot.bounced.push(next);
        shot.target = next;
        self.projectile.insert(missile, shot);
        true
    }

    /// Points an entity at the cast it was ordered to make.
    pub fn order_cast(&mut self, entity: Entity, cast: PendingCast) {
        self.casting.insert(entity, cast);
    }
}
