//! Throwing a hook, and what it drags back.

use bota_proto::{DamageKind, Fixed, Target, UnitKind, Vec2};

use crate::engine::Entity;
use crate::game::{Hook, Modifier, ModifierKind, Transform, World, ability, is_structure, rules};
use crate::game::{
    clamp_to_map, facing_towards, heading_of, isqrt64, move_towards, per_tick, point_along,
};

impl World {
    /// Throws a hook the way of a spot, out to its whole reach.
    pub fn cast_hook(&mut self, caster: Entity, level: usize, target: Target) -> bool {
        let Target::Pos(pos) = target else {
            return false;
        };
        let (Some(from), Some(facing), Some(side)) = (
            self.transform.get(caster).map(|t| t.pos),
            self.transform.get(caster).map(|t| t.facing),
            self.team.get(caster).copied(),
        ) else {
            return false;
        };
        let reach = rules::units(rules::HOOK_RANGE);
        // A spot under the caster's own feet gives no way to throw; the
        // hook goes the way the caster faces.
        let towards = if pos == from {
            from + heading_of(facing)
        } else {
            pos
        };
        let aim = clamp_to_map(point_along(from, towards, reach));
        let hook = self.spawn();
        self.transform.insert(
            hook,
            Transform {
                pos: from,
                facing: facing_towards(from, aim),
            },
        );
        self.set_team(hook, side);
        self.hook.insert(
            hook,
            Hook {
                owner: caster,
                aim,
                speed: Fixed::from_int(rules::HOOK_SPEED),
                reach_left: reach,
                radius: rules::units(rules::HOOK_RADIUS),
                damage: rules::HOOK_DAMAGE[level],
                caught: None,
                returning: false,
                links: [None; rules::HOOK_LINKS],
            },
        );
        true
    }

    /// Lays a hook's chain out from the hook back towards the thrower: a
    /// link every gap's length behind the hook for as much chain as is
    /// out, so links come out of the thrower as the hook flies and go back
    /// into the thrower as it returns.
    fn lay_chain(&mut self, hook: &mut Hook, home: Vec2, at: Vec2) {
        let gap = i64::from(rules::units(rules::HOOK_LINK_GAP).raw);
        let out = isqrt64(home.distance_squared(at));
        let count = ((out / gap) as usize).min(rules::HOOK_LINKS);
        for k in 0..rules::HOOK_LINKS {
            let wanted = k < count;
            match (wanted, hook.links[k]) {
                (true, link) => {
                    let spot = point_along(
                        at,
                        home,
                        Fixed {
                            raw: (gap * (k as i64 + 1)) as i32,
                        },
                    );
                    match link {
                        Some(link) => {
                            if let Some(transform) = self.transform.get_mut(link) {
                                transform.pos = spot;
                            }
                        }
                        None => {
                            hook.links[k] =
                                Some(self.spawn_mark(ability::MEAT_HOOK, hook.owner, spot, 0));
                        }
                    }
                }
                (false, Some(link)) => {
                    self.take_mark(link);
                    hook.links[k] = None;
                }
                (false, None) => {}
            }
        }
    }

    /// Runs every hook one tick on.
    ///
    /// A hook whose thrower has fallen is given up where it is, and lets go of
    /// whatever it was dragging.
    pub fn tick_hooks(&mut self) {
        let entities = self.take_entity_snapshot();
        for entity in entities.iter().copied() {
            let Some(mut hook) = self.hook.get(entity).copied() else {
                continue;
            };
            let (Some(at), Some(home)) = (
                self.transform.get(entity).map(|t| t.pos),
                self.transform.get(hook.owner).map(|t| t.pos),
            ) else {
                self.let_go(entity);
                continue;
            };
            if !self.alive(hook.owner) {
                self.let_go(entity);
                continue;
            }
            let step = per_tick(hook.speed);
            if hook.returning {
                // With something caught the hook stops where the two
                // bodies meet, the catch set down in front of the thrower;
                // empty it comes all the way in.
                let landing = hook.caught.map_or(Fixed::ZERO, |caught| {
                    self.hull
                        .get(hook.owner)
                        .map_or(Fixed::ZERO, |h| h.collision)
                        + self.hull.get(caught).map_or(Fixed::ZERO, |h| h.collision)
                        + rules::units(rules::STEER_MARGIN)
                });
                let next = move_towards(at, home, step);
                if next.within(home, landing) {
                    let rest = point_along(home, at, landing);
                    if let Some(caught) = hook.caught {
                        self.drag(entity, caught, rest);
                    }
                    self.let_go(entity);
                    continue;
                }
                self.put_at(entity, next, home);
                self.lay_chain(&mut hook, home, next);
                if let Some(caught) = hook.caught {
                    self.drag(entity, caught, next);
                }
                self.hook.insert(entity, hook);
                continue;
            }
            let next = move_towards(at, hook.aim, step);
            hook.reach_left -= step;
            self.put_at(entity, next, hook.aim);
            self.lay_chain(&mut hook, home, next);
            if let Some(caught) = self.caught_at(entity, hook.owner, next, hook.radius) {
                hook.caught = Some(caught);
                hook.returning = true;
                if self.team.get(caught).copied() != self.team.get(hook.owner).copied() {
                    self.push_hit(Some(hook.owner), caught, hook.damage, DamageKind::Pure);
                }
            } else if next == hook.aim || hook.reach_left <= Fixed::ZERO {
                hook.returning = true;
            }
            self.hook.insert(entity, hook);
        }
        self.recycle_entity_snapshot(entities);
    }

    /// What a hook catches where it now flies.
    ///
    /// The nearest body in its way that is neither the one who threw it nor
    /// anything rooted to the map: a hook takes units, not buildings.
    fn caught_at(&self, hook: Entity, owner: Entity, at: Vec2, radius: Fixed) -> Option<Entity> {
        let mut best: Option<(i64, Entity)> = None;
        for other in self.entities.iter() {
            if other == hook || other == owner || !self.alive(other) {
                continue;
            }
            let Some(kind) = self.kind.get(other).copied() else {
                continue;
            };
            if is_structure(kind) || kind == UnitKind::Ward {
                continue;
            }
            let Some(spot) = self.transform.get(other).map(|t| t.pos) else {
                continue;
            };
            let hulls = self.hull.get(other).map_or(Fixed::ZERO, |hull| hull.bound);
            if !spot.within(at, radius + hulls) {
                continue;
            }
            let far = at.distance_squared(spot);
            if best.is_none_or(|(nearest, _)| far < nearest) {
                best = Some((far, other));
            }
        }
        best.map(|(_, caught)| caught)
    }

    /// Moves a hook and turns it the way it is going.
    fn put_at(&mut self, hook: Entity, to: Vec2, facing_at: Vec2) {
        if let Some(at) = self.transform.get_mut(hook) {
            at.facing = facing_towards(at.pos, facing_at);
            at.pos = to;
        }
    }

    /// Drags what a hook caught to a spot, and holds it there.
    ///
    /// The hold is handed out afresh every tick, so it lifts on its own once
    /// the hook lets go.
    fn drag(&mut self, hook: Entity, caught: Entity, to: Vec2) {
        if let Some(at) = self.transform.get_mut(caught) {
            at.pos = to;
        }
        self.put_modifier(
            caught,
            Modifier {
                kind: ModifierKind::Stunned,
                source: Some(hook),
                ticks_left: Some(2),
            },
        );
        self.forget_walk(caught);
    }

    /// Takes a hook and its chain out of the world.
    fn let_go(&mut self, hook: Entity) {
        if let Some(flown) = self.hook.get(hook).copied() {
            for link in flown.links.into_iter().flatten() {
                self.take_mark(link);
            }
        }
        self.hook.remove(hook);
        self.transform.remove(hook);
        self.team.remove(hook);
        self.despawn(hook);
    }
}
