//! Picking a target, and clearing away what has fallen.

use bota_proto::{EventKind, Fixed, Team, UnitKind};

use crate::engine::{Entity, World, is_structure, wire_id};
use crate::sim::{Event, EventVisibility};

impl World {
    /// Each entity that can attack takes the best hostile in reach of its
    /// acquisition, and keeps it while it lives and stays in range.
    ///
    /// What is taken on depends on the order in hand: one told to walk
    /// somewhere or to stand takes on nothing, one told to attack takes on
    /// that and nothing else, and one left idle, holding, or walking to attack
    /// looks for the best thing in reach of its acquisition.
    ///
    /// A lane creep is left out: it has a mind of its own, which runs first.
    pub fn acquire_targets(&mut self) {
        for entity in self.entities.iter().collect::<Vec<_>>() {
            if self.attacking.get(entity).is_none() || self.lane_ai.get(entity).is_some() {
                continue;
            }
            // The order decides whether anything is taken on at all.
            match self.orders.get(entity).map(|o| o.current) {
                // Named outright, and held however far it runs.
                Some(crate::engine::UnitOrder::Attack { target, .. }) => {
                    if self.may_attack_on_order(entity, target) {
                        self.set_target(entity, target);
                    } else {
                        self.target.remove(entity);
                    }
                    continue;
                }
                // Told to walk somewhere, it walks; told to stand, it
                // stands. Whatever it passes, or whatever passes it, is
                // nothing to it either way.
                Some(crate::engine::UnitOrder::Move { .. } | crate::engine::UnitOrder::Stand) => {
                    self.target.remove(entity);
                    continue;
                }
                _ => {}
            }
            if let Some(held) = self.target_of(entity)
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
                    self.set_target(entity, found);
                }
                None => {
                    self.target.remove(entity);
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

    /// Turns every swing that came due into a hit or a missile in the air.
    ///
    /// The cycle itself is worked out before this, by [`attacking_system`];
    /// putting a missile in the world is a change of who exists, so it lives
    /// here.
    ///
    /// [`attacking_system`]: crate::engine::attacking_system
    /// Tells each side what it was near enough to feel.
    pub fn tell_of(&self, felt: &[crate::engine::Landed], events: &mut Vec<Event>) {
        for blow in felt {
            events.push(Event {
                kind: EventKind::Damaged {
                    source: blow.source.map(wire_id),
                    target: wire_id(blow.target),
                    amount: blow.amount,
                    kind: blow.kind,
                    crit: false,
                },
                visible_to: self.who_may_know(blow.at, blow.side),
            });
        }
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
