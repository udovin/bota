//! What a lane creep does about the fight it finds itself in.
//!
//! It gives chase only so long, comes back to where it left its route, and
//! looks again for something better whenever it is not held to what it has.

use bota_proto::Fixed;

use crate::engine::{Entity, LaneAi, World};
use crate::sim::rules;

impl World {
    /// Runs one tick of every lane creep's mind.
    ///
    /// A creep with a target keeps it while the chase lasts and the target is
    /// worth keeping; the ranking runs again whenever nothing holds it. Losing
    /// a target leaves it walking to where the target was last seen, and from
    /// there back to the route it left.
    pub fn tick_lane_ai(&mut self) {
        for entity in self.entities.iter().collect::<Vec<_>>() {
            let Some(mut ai) = self.lane_ai.get(entity).copied() else {
                continue;
            };
            ai.provoked = ai.provoked.saturating_sub(1);
            let held = self.target_of(entity).filter(|t| self.alive(*t));
            let reach = self
                .stats
                .get(entity)
                .map_or(Fixed::ZERO, |s| s.attack_range);
            let in_reach = held.is_some_and(|t| self.reachable(entity, reach, t));
            if let Some(target) = held {
                if in_reach {
                    ai.chase_left = rules::CREEP_CHASE_TICKS;
                } else {
                    ai.chase_left = ai.chase_left.saturating_sub(1);
                }
                if let Some(at) = self.transform.get(target).map(|t| t.pos) {
                    ai.last_seen = Some(at);
                }
                if ai.chase_left == 0 && ai.provoked == 0 {
                    self.target.remove(entity);
                }
            }
            let looking = self.target_of(entity);
            let outranked = looking.is_some_and(|t| self.outranked(entity, t, reach));
            if (looking.is_none() || (ai.provoked == 0 && !in_reach) || outranked)
                && let Some(found) = self.acquire(
                    entity,
                    self.stats
                        .get(entity)
                        .map_or(Fixed::ZERO, |s| s.acquisition),
                    self.priority_of(entity),
                )
            {
                if self.target_of(entity) != Some(found) && ai.anchor.is_none() {
                    ai.anchor = self.transform.get(entity).map(|t| t.pos);
                }
                self.set_target(entity, found);
                ai.chase_left = rules::CREEP_CHASE_TICKS;
            }
            if self.target.get(entity).is_none() {
                self.clear_reached(entity, &mut ai);
            }
            self.lane_ai.insert(entity, ai);
        }
    }

    /// Whether something better than the held target now stands in reach.
    fn outranked(&self, seeker: Entity, held: Entity, reach: Fixed) -> bool {
        let Some(best) = self.acquire(seeker, reach, self.priority_of(seeker)) else {
            return false;
        };
        best != held
    }

    /// Clears a mark the creep has reached, so the next one takes over.
    ///
    /// Where a creep walks is [`World::march_lanes`]; this only decides what
    /// is still worth walking to.
    fn clear_reached(&mut self, entity: Entity, ai: &mut LaneAi) {
        let radius = rules::units(rules::LANE_WAYPOINT_RADIUS);
        let Some(at) = self.transform.get(entity).map(|t| t.pos) else {
            return;
        };
        if ai.last_seen.is_some_and(|spot| at.within(spot, radius)) {
            ai.last_seen = None;
        }
        if ai.anchor.is_some_and(|spot| at.within(spot, radius)) {
            ai.anchor = None;
        }
    }

    /// Hands a creep the target an attack order pointed it at, and holds it
    /// there for a while.
    ///
    /// Ordering an attack at one of your own does not hand the creep anything:
    /// it ranks everyone else first and takes the one who gave the order only
    /// when there is nobody else.
    pub fn provoke(&mut self, creep: Entity, orderer: Entity, at_own: bool) {
        let Some(mut ai) = self.lane_ai.get(creep).copied() else {
            return;
        };
        if self.orders.get(creep).is_some() && ai.anchor.is_none() {
            ai.anchor = self.transform.get(creep).map(|t| t.pos);
        }
        let reach = self.stats.get(creep).map_or(Fixed::ZERO, |s| s.acquisition);
        let order = self.priority_of(creep);
        let taken = if at_own {
            self.acquire_demoting(creep, reach, order, Some(orderer))
        } else {
            Some(orderer)
        };
        match taken {
            Some(target) => {
                self.set_target(creep, target);
                ai.chase_left = rules::CREEP_CHASE_TICKS;
                ai.provoked = if target == orderer && !at_own {
                    rules::ORDER_AGGRO_HOLD_TICKS
                } else {
                    0
                };
            }
            None => {
                self.target.remove(creep);
            }
        }
        self.lane_ai.insert(creep, ai);
    }
}
