//! Walking: turning towards where an entity is going, then taking its step.

use bota_proto::Vec2;

use crate::engine::{Entity, Route, UnitOrder, World};
use crate::sim::{facing_gap, facing_towards, find_path, grid_los, per_tick, rules, turn_towards};

impl World {
    /// Turns and steps everything that has somewhere to be.
    ///
    /// What an entity is set on decides first: with its target in reach it
    /// stands still and comes round to it, out of reach it walks at it. Only
    /// with nothing to fight does it walk where it was told.
    ///
    /// Turning comes first and costs the tick: an entity more than
    /// [`rules::TURN_TOLERANCE_BRADS`] off the way it wants to face stands
    /// still until it has come round. A creep marches round what is in its
    /// way; anything a player drives slides along it.
    pub fn walk_bodies(&mut self) {
        for entity in self.entities.iter().collect::<Vec<_>>() {
            // What it is set on comes before where it was told to go: in
            // reach it stands and comes round, out of reach it closes.
            let on_target = self
                .target_of(entity)
                .filter(|on| self.alive(*on))
                .and_then(|on| {
                    let at = self.transform.get(on)?.pos;
                    Some((at, self.in_reach(entity, on)))
                });
            let holding = matches!(
                self.orders.get(entity).map(|o| o.current),
                Some(UnitOrder::Hold)
            );
            let (dest, facing_only) = match on_target {
                // Holding, it comes round to what it is set on but never
                // leaves the spot it was left on.
                Some((at, in_reach)) => (at, in_reach || holding),
                None => {
                    let Some(dest) = self
                        .orders
                        .get(entity)
                        .and_then(|o| destination(&o.current))
                    else {
                        continue;
                    };
                    (dest, false)
                }
            };
            let (Some(from), Some(stats)) = (
                self.transform.get(entity).map(|t| t.pos),
                self.stats.get(entity).copied(),
            ) else {
                continue;
            };
            if from == dest {
                continue;
            }
            if facing_only {
                let wanted = facing_towards(from, dest);
                if let Some(stats) = self.stats.get(entity).copied()
                    && let Some(transform) = self.transform.get_mut(entity)
                {
                    transform.facing = turn_towards(transform.facing, wanted, stats.turn_rate);
                }
                continue;
            }
            let waypoint = self.next_corner(entity, from, dest);
            let step = per_tick(stats.move_speed);
            let marching = self.march.get(entity).is_some();
            let (aim, trace) = if marching {
                let held = self.march.get(entity).and_then(|m| m.trace);
                self.march_aim(entity, waypoint, step, held)
            } else {
                (waypoint, None)
            };
            let wanted = facing_towards(from, aim);
            let facing = turn_towards(
                self.transform.get(entity).expect("looked up above").facing,
                wanted,
                stats.turn_rate,
            );
            let mut next = from;
            if facing_gap(facing, wanted) <= rules::TURN_TOLERANCE_BRADS {
                next = if marching {
                    self.march_step(entity, aim, step)
                } else {
                    self.walk_step(entity, aim, step)
                };
            }
            if let Some(mut march) = self.march.get(entity).copied() {
                march.shove = if next == from {
                    march.shove.saturating_add(1)
                } else {
                    march.shove.saturating_sub(1)
                };
                march.trace = trace;
                self.march.insert(entity, march);
            }
            if let Some(transform) = self.transform.get_mut(entity) {
                transform.facing = facing;
                transform.pos = next;
            }
        }
    }

    /// The corner to walk at next: the destination itself when it is in plain
    /// sight, otherwise the next corner of a route laid round the buildings.
    ///
    /// Only what a player drives keeps a route; a creep walks the lane it was
    /// given and never plans around anything.
    fn next_corner(&mut self, entity: Entity, from: Vec2, dest: Vec2) -> Vec2 {
        if self.march.get(entity).is_some() {
            return dest;
        }
        let mut route = self.route.remove(entity).unwrap_or(Route {
            path: Vec::new(),
            goal: dest,
        });
        if !route.goal.within(dest, rules::units(rules::REPATH_DRIFT)) {
            route.path.clear();
        }
        route.goal = dest;
        while route
            .path
            .first()
            .is_some_and(|corner| from.within(*corner, rules::units(rules::WAYPOINT_RADIUS)))
        {
            route.path.remove(0);
        }
        if route.path.is_empty() && !grid_los(&self.grid, from, dest) {
            route.path = find_path(&self.grid, from, dest);
        }
        let next = route.path.first().copied().unwrap_or(dest);
        self.route.insert(entity, route);
        next
    }

    /// Whether an entity is marching a lane rather than being driven.
    pub fn is_marching(&self, entity: Entity) -> bool {
        self.march.get(entity).is_some()
    }
}

/// Where an order sends an entity, if it sends it anywhere.
fn destination(order: &UnitOrder) -> Option<Vec2> {
    match order {
        UnitOrder::Move { pos } | UnitOrder::AttackMove { pos } => Some(*pos),
        UnitOrder::Idle | UnitOrder::Stand | UnitOrder::Hold | UnitOrder::Attack { .. } => None,
    }
}
