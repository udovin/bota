//! Picking a target, and clearing away what has fallen.

use bota_proto::{EventKind, Fixed, Team, UnitKind};

use crate::game::{Entity, UnitOrder, World, is_structure, rules, wire_id};
use crate::game::{Event, EventVisibility};

impl World {
    /// Each entity that can attack takes the best hostile in reach of its
    /// acquisition, and keeps it while it lives and stays in range.
    ///
    /// Whether an entity is still standing.
    pub fn alive(&self, entity: Entity) -> bool {
        self.entities.contains(entity)
            && self.health.get(entity).is_some_and(|h| h.hp > Fixed::ZERO)
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
        let hulls = self.hull.get(attacker).map_or(Fixed::ZERO, |h| h.bound)
            + self.hull.get(target).map_or(Fixed::ZERO, |h| h.bound);
        at.pos.within(their_at.pos, stats.attack_range + hulls)
    }

    /// Tells each side what it was near enough to feel.
    pub fn tell_of(&self, felt: &[crate::game::Landed], events: &mut Vec<Event>) {
        for blow in felt {
            events.push(Event {
                kind: EventKind::Damaged {
                    source: blow.source.map(wire_id),
                    target: wire_id(blow.target),
                    amount: blow.amount,
                    kind: blow.kind,
                    crit: blow.crit,
                },
                visible_to: self.who_may_know(blow.at, blow.side),
            });
        }
    }

    /// Tells each side of the attacks it was near enough to see miss.
    pub fn tell_of_misses(&self, missed: &[crate::game::Missed], events: &mut Vec<Event>) {
        for miss in missed {
            events.push(Event {
                kind: EventKind::Missed {
                    source: miss.source.map(wire_id),
                    target: wire_id(miss.target),
                },
                visible_to: self.who_may_know(miss.at, miss.side),
            });
        }
    }

    /// Hands on the fights and follows aimed at a fallen entity.
    ///
    /// An attack order degrades to attack-moving at the spot the target was
    /// last seen, so the fight carries on with whatever acquisition finds
    /// there. A follow ends where the one followed fell.
    fn carry_fights_on(&mut self, fallen: Entity) {
        for follower in self.entities.iter().collect::<Vec<_>>() {
            match self.orders.get(follower).map(|o| o.current) {
                Some(UnitOrder::Follow { target, last_seen }) if target == fallen => {
                    self.set_order(follower, UnitOrder::Move { pos: last_seen });
                }
                Some(UnitOrder::Attack { target, last_seen }) if target == fallen => {
                    self.set_order(follower, UnitOrder::AttackMove { pos: last_seen });
                }
                _ => {}
            }
        }
    }

    /// Clears away what has fallen and tells who may know, then judges
    /// whether the match is decided.
    pub fn bury(&mut self, fallen: Vec<(Entity, Option<Entity>)>, events: &mut Vec<Event>) {
        let mut structure_fell = false;
        // Which side lost a tower this tick, and which its Ancient; Radiant
        // first.
        let mut lost_tower = [false; 2];
        let mut lost_ancient = [false; 2];
        for (entity, killer) in fallen {
            // Two blows that both kill bury once: what is gone, or already
            // lies fallen, is past burying.
            if !self.entities.contains(entity) || self.lies_fallen(entity) {
                continue;
            }
            if let Some((kind, side)) = self.bury_entity(entity, killer, events) {
                structure_fell = true;
                let at = match side {
                    Team::Radiant => 0,
                    Team::Dire => 1,
                    Team::Neutral => continue,
                };
                match kind {
                    UnitKind::Tower => lost_tower[at] = true,
                    UnitKind::Ancient => lost_ancient[at] = true,
                    _ => {}
                }
            }
        }
        if structure_fell {
            self.lay_passability();
        }
        self.judge(lost_tower, lost_ancient);
    }

    fn bury_entity(
        &mut self,
        entity: Entity,
        killer: Option<Entity>,
        events: &mut Vec<Event>,
    ) -> Option<(UnitKind, Team)> {
        assert!(self.entities.contains(entity));
        self.cancel_action(entity);
        self.rot_goes_out(entity);
        self.carry_fights_on(entity);
        self.feed_flesh_heaps(entity);
        self.feed_souls(entity, killer);
        let kind = self.kind.get(entity).copied();
        let side = self.team.get(entity).copied();
        let denied = killer
            .and_then(|k| self.team.get(k).copied())
            .is_some_and(|theirs| Some(theirs) == side);
        let paid = self.pay_for(entity, killer, events);
        let at = self
            .transform
            .get(entity)
            .map_or(bota_proto::Vec2::ZERO, |t| t.pos);
        events.push(Event {
            kind: EventKind::Died {
                unit: wire_id(entity),
                killer: killer.map(wire_id),
                denied,
                gold: paid,
            },
            visible_to: self.who_may_know(at, side.unwrap_or(Team::Neutral)),
        });
        let structure = kind.zip(side).filter(|(kind, _)| is_structure(*kind));
        if let Some((_, side)) = structure {
            events.push(Event {
                kind: EventKind::StructureDestroyed {
                    unit: wire_id(entity),
                    team: side,
                },
                visible_to: EventVisibility::Everyone,
            });
        }
        self.bury_seat(entity);
        if let Some(index) = killer.and_then(|k| self.seats.iter().position(|s| s.unit == Some(k)))
            && kind == Some(UnitKind::Hero)
            && !denied
        {
            self.seats[index].kills += 1;
        }
        // A seat's hero lies where it fell, handle and all, until it stands
        // again; everything else leaves the world.
        if !self.lies_fallen(entity) {
            self.despawn(entity);
            assert!(!self.entities.contains(entity));
        }
        structure
    }

    /// Whether an entity is a seat's hero lying fallen.
    pub fn lies_fallen(&self, entity: Entity) -> bool {
        self.seats.iter().any(|seat| seat.fallen == Some(entity))
    }

    fn bury_seat(&mut self, entity: Entity) {
        for index in 0..self.seats.len() {
            if self.seats[index].unit == Some(entity) {
                let level = self.seats[index].level;
                self.let_souls_go(entity);
                self.lay_down_hero(entity);
                self.seats[index].unit = None;
                self.seats[index].fallen = Some(entity);
                self.seats[index].deaths += 1;
                self.seats[index].respawn_left = World::respawn_wait(level);
            }
            if self.seats[index].courier == Some(entity) {
                self.seats[index].courier_kept = self.inventory.remove(entity);
            }
        }
    }

    /// Decides the match once a side has lost, after everything that fell
    /// on the tick is buried.
    ///
    /// A side loses with its Ancient, with any tower when the rules say so,
    /// and once its heroes have died as often as the rules allow. Both sides
    /// losing on one tick is a draw, and so is the end of the time the rules
    /// give, whatever else that tick held.
    fn judge(&mut self, lost_tower: [bool; 2], lost_ancient: [bool; 2]) {
        if self.winner.is_some() {
            return;
        }
        let ruled = self.rules;
        let deaths = |team| {
            self.seats
                .iter()
                .filter(|seat| seat.team == team)
                .fold(0u16, |total, seat| total.saturating_add(seat.deaths))
        };
        let loses = |at: usize, team: Team| {
            lost_ancient[at]
                || (ruled.tower_ends_it && lost_tower[at])
                || (ruled.death_limit > 0 && deaths(team) >= ruled.death_limit)
        };
        let lost = [loses(0, Team::Radiant), loses(1, Team::Dire)];
        self.winner = match (self.out_of_time(), lost) {
            (true, _) | (false, [true, true]) => Some(Team::Neutral),
            (false, [true, false]) => Some(Team::Dire),
            (false, [false, true]) => Some(Team::Radiant),
            (false, [false, false]) => None,
        };
    }

    /// Whether the time the rules give the match has run out.
    fn out_of_time(&self) -> bool {
        self.rules.time_limit > 0 && self.tick >= rules::PREGAME_TICKS + self.rules.time_limit
    }

    /// Whether the match is decided, sealing the draw first when its time
    /// ran out.
    pub(crate) fn decided(&mut self) -> bool {
        if self.out_of_time() {
            self.winner.get_or_insert(Team::Neutral);
        }
        self.winner.is_some()
    }
}
