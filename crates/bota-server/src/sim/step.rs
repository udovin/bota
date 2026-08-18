//! One tick of the world, in its fixed order.

use bota_proto::{EntityId, EventKind, Fixed, Order, RejectReason, SlotId, Team, UnitKind, Vec2};

use crate::sim::{
    UnitOrder, World, blocked_by_stander, blocked_by_units, clamp_to_map, facing_gap,
    facing_towards, find_path, grid_los, in_attack_range, move_towards, per_tick, rules,
    steer_target, turn_towards, unit_slide,
};

/// One accepted order, translated to a seat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    /// Which seat issued it.
    pub slot: SlotId,
    /// What it asks for.
    pub order: Order,
}

/// Who may learn that an event happened.
///
/// Spectators and the replay always see everything; this limits the player
/// streams.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventVisibility {
    /// Both teams.
    Everyone,
    /// One team only.
    OneTeam(Team),
}

/// One thing that happened during a tick, with its audience.
#[derive(Clone, Debug)]
pub struct Event {
    /// What happened.
    pub kind: EventKind,
    /// Who may know.
    pub visible_to: EventVisibility,
}

/// Whether an allied unit is low enough to be denied.
fn deniable(unit: &crate::sim::Unit) -> bool {
    unit.is_creep()
        && i64::from(unit.hp) * 100 < i64::from(unit.max_hp) * i64::from(rules::DENY_HP_PCT)
}

impl World {
    /// Whether a seat may issue this order right now, and why not.
    pub fn validate(&self, slot: SlotId, order: &Order) -> Result<(), RejectReason> {
        if self.winner.is_some() {
            return Err(RejectReason::NotPlaying);
        }
        let Some(seat) = self.seat(slot) else {
            return Err(RejectReason::NotYourSlot);
        };
        let Some(unit_id) = seat.unit else {
            return Err(RejectReason::HeroDead);
        };
        match order {
            Order::Stop | Order::HoldPosition | Order::Move { .. } | Order::AttackMove { .. } => {
                Ok(())
            }
            Order::AttackUnit { target } => {
                if !self.can_see(seat.team, *target) {
                    return Err(RejectReason::UnknownTarget);
                }
                let target_unit = self.units.get(*target).expect("can_see checked existence");
                if target_unit.invulnerable {
                    return Err(RejectReason::WrongTargetKind);
                }
                Ok(())
            }
            Order::CastAbility { slot: ab, target } => {
                self.validate_cast(slot, unit_id, *ab, target)
            }
            Order::UseItem { slot: item, .. } => self.validate_use(slot, *item),
            Order::SellItem { slot: item } => self.validate_sell(slot, unit_id, *item),
            Order::MoveItem { from, to } => self.validate_move(slot, unit_id, *from, *to),
            Order::LevelUpAbility { slot: ab } => self.validate_level_up(slot, *ab),
            Order::BuyItem { item } => self.validate_buy(slot, unit_id, *item),
        }
    }

    /// Advances the world one tick.
    ///
    /// The order of phases is fixed; changing it invalidates every recorded
    /// replay and hash baseline. Commands must already be validated and at most
    /// one per seat.
    pub fn step(&mut self, cmds: &[Command]) -> Vec<Event> {
        let mut events = Vec::new();
        if self.winner.is_some() {
            return events;
        }
        self.tick += 1; //                                      1. time
        self.apply_commands(cmds); //                           2. orders
        self.degrade_fogged_orders(); //                        2. orders vs fog
        self.spawn_waves(); //                                  3. scheduled
        self.spawn_neutrals(); //                               3. scheduled
        self.tick_roshan(); //                                  3. scheduled
        self.tick_respawns(); //                                3. scheduled
        self.passive_gold(); //                                 3. scheduled
        self.regen(); //                                        4. statuses
        self.tick_cooldowns(); //                               4. statuses
        self.tick_items(); //                                   4. statuses
        self.aggro(); //                                        5. target choice
        self.execute_movement(); //                             6. movement
        let mut dmg = Vec::new();
        self.run_attacks(&mut dmg); //                          7. attacks
        self.move_projectiles(&mut dmg); //                     7. projectiles
        self.run_casts(&mut events); //                         8. abilities
        let deaths = self.resolve_damage(dmg, &mut events); //  9. damage
        self.process_deaths(deaths, &mut events); //           10. deaths, victory
        //                                                     11. vision is derived on demand
        events //                                              12-13. done
    }

    /// The audience of an event at a point involving a unit of `involved`.
    pub fn point_visibility(&self, pos: Vec2, involved: Team) -> EventVisibility {
        let radiant = involved == Team::Radiant || self.can_see_point(Team::Radiant, pos);
        let dire = involved == Team::Dire || self.can_see_point(Team::Dire, pos);
        match (radiant, dire) {
            (true, true) => EventVisibility::Everyone,
            (true, false) => EventVisibility::OneTeam(Team::Radiant),
            (false, true) => EventVisibility::OneTeam(Team::Dire),
            (false, false) => EventVisibility::OneTeam(involved),
        }
    }

    fn apply_commands(&mut self, cmds: &[Command]) {
        let mut sorted: Vec<&Command> = cmds.iter().collect();
        sorted.sort_by_key(|c| c.slot.0);
        for cmd in sorted {
            let Some(seat) = self.seat(cmd.slot) else {
                continue;
            };
            let Some(unit_id) = seat.unit else {
                continue;
            };
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            // Any order is also an animation cancel: the backswing ends now.
            unit.recovering = 0;
            match cmd.order {
                Order::Stop => {
                    unit.order = UnitOrder::Idle;
                    unit.engage = None;
                    unit.windup = None;
                }
                Order::HoldPosition => {
                    unit.order = UnitOrder::Hold;
                    unit.engage = None;
                    unit.windup = None;
                }
                Order::Move { pos } => {
                    unit.order = UnitOrder::Move {
                        pos: clamp_to_map(pos),
                    };
                    unit.engage = None;
                    unit.windup = None;
                }
                Order::AttackMove { pos } => {
                    unit.order = UnitOrder::AttackMove {
                        pos: clamp_to_map(pos),
                    };
                }
                Order::AttackUnit { target } => {
                    let info = self.units.get(target).map(|t| (t.pos, t.team));
                    let my_team = self.units.get(unit_id).map(|u| u.team);
                    if let (Some((last_seen, target_team)), Some(my_team)) = (info, my_team) {
                        let friendly = target_team == my_team;
                        {
                            let unit = self
                                .units
                                .get_mut(unit_id)
                                .expect("borrow released and id unchanged");
                            unit.order = UnitOrder::Attack { target, last_seen };
                            // Against an ally the aggro pass decides between
                            // following and denying, every tick anew.
                            unit.engage = if friendly { None } else { Some(target) };
                        }
                        if friendly {
                            self.deaggro_call(unit_id);
                        } else {
                            self.provoke_creeps(unit_id, target);
                        }
                    }
                }
                Order::CastAbility { slot, target } => {
                    unit.pending_cast = Some(crate::sim::PendingCast { slot, target });
                }
                Order::LevelUpAbility { slot } => {
                    self.apply_level_up(cmd.slot, slot);
                }
                Order::UseItem { slot: item, .. } => {
                    self.apply_use(cmd.slot, unit_id, item);
                }
                Order::BuyItem { item } => {
                    self.apply_buy(cmd.slot, unit_id, item);
                }
                Order::SellItem { slot: item } => {
                    self.apply_sell(cmd.slot, unit_id, item);
                }
                Order::MoveItem { from, to } => {
                    self.apply_move(cmd.slot, unit_id, from, to);
                }
            }
        }
    }

    /// Ends attack orders whose target died, and degrades those whose target
    /// fell into fog: a unit never acts on what its team cannot see, so the
    /// fogged case turns into attack-moving at the last seen spot.
    fn degrade_fogged_orders(&mut self) {
        for id in self.units.ids() {
            let Some(unit) = self.units.get(id) else {
                continue;
            };
            let UnitOrder::Attack { target, last_seen } = unit.order else {
                continue;
            };
            let team = unit.team;
            let dead = self.units.get(target).is_none_or(|t| t.hp <= 0);
            let visible = !dead
                && self
                    .units
                    .get(target)
                    .is_some_and(|t| t.team == team || self.can_see_point(team, t.pos));
            if dead {
                // A hero's fight rolls onto the closest enemy in acquisition
                // range: the switch happens only mid-attack, never from rest.
                let next = if unit.kind == UnitKind::Hero {
                    self.pick_target(id, Some(rules::units(rules::ACQUISITION_RANGE)))
                        .and_then(|n| self.units.get(n).map(|t| (n, t.pos)))
                } else {
                    None
                };
                let unit = self.units.get_mut(id).expect("iterating live ids");
                match next {
                    Some((next, last_seen)) => {
                        unit.order = UnitOrder::Attack {
                            target: next,
                            last_seen,
                        };
                        unit.engage = Some(next);
                    }
                    None => {
                        unit.order = UnitOrder::Idle;
                        if unit.engage == Some(target) {
                            unit.engage = None;
                        }
                    }
                }
            } else if visible {
                let pos = self.units.get(target).expect("checked above").pos;
                let unit = self.units.get_mut(id).expect("iterating live ids");
                unit.order = UnitOrder::Attack {
                    target,
                    last_seen: pos,
                };
            } else {
                let unit = self.units.get_mut(id).expect("iterating live ids");
                unit.order = UnitOrder::AttackMove { pos: last_seen };
                if unit.engage == Some(target) {
                    unit.engage = None;
                }
            }
        }
    }

    fn tick_cooldowns(&mut self) {
        for (_, unit) in self.units.iter_mut() {
            unit.attack_cooldown = unit.attack_cooldown.saturating_sub(1);
            unit.recovering = unit.recovering.saturating_sub(1);
            unit.provoked_ticks = unit.provoked_ticks.saturating_sub(1);
            unit.aggro_cooldown = unit.aggro_cooldown.saturating_sub(1);
        }
        self.tick_ability_cooldowns();
    }

    /// A hero's attack aimed at an enemy hero calls the victim's creeps near
    /// the attacker, and the victim's towers the attacker stands in reach of,
    /// onto the attacker. Fired by both the order and every swing; each creep
    /// or tower answers at most once per call cooldown.
    pub(crate) fn provoke_creeps(&mut self, attacker: EntityId, target: EntityId) {
        let call = {
            let attacker_unit = self.units.get(attacker);
            let victim = self.units.get(target);
            match (attacker_unit, victim) {
                (Some(a), Some(v))
                    if a.kind == UnitKind::Hero && v.kind == UnitKind::Hero && v.team != a.team =>
                {
                    Some((a.pos, a.radius, v.team))
                }
                _ => None,
            }
        };
        let Some((around, attacker_radius, team)) = call else {
            return;
        };
        let radius = rules::units(rules::AGGRO_CALL_RADIUS);
        for id in self.units.ids() {
            let Some(u) = self.units.get(id) else {
                continue;
            };
            if u.team != team || u.aggro_cooldown > 0 {
                continue;
            }
            let called = if u.is_creep() {
                u.pos.within(around, radius)
            } else if u.kind == UnitKind::Tower {
                u.pos
                    .within(around, u.attack_range + u.radius + attacker_radius)
            } else {
                false
            };
            if called {
                let u = self.units.get_mut(id).expect("iterating live ids");
                u.engage = Some(attacker);
                u.provoked_ticks = rules::CREEP_PROVOKE_TICKS;
                u.aggro_cooldown = rules::AGGRO_CALL_COOLDOWN_TICKS;
                u.shunned = None;
                u.returning = false;
            }
        }
    }

    /// An order aimed at an ally calls enemy creeps and towers off the
    /// orderer. The call works at any time — the call cooldown only limits
    /// calls onto a unit — and the called-off unit refuses to re-acquire the
    /// orderer by proximity until that cooldown runs out, or standing among
    /// the chasers would undo the call on the next tick.
    fn deaggro_call(&mut self, orderer: EntityId) {
        let Some(o) = self.units.get(orderer) else {
            return;
        };
        if o.kind != UnitKind::Hero {
            return;
        }
        let (around, orderer_radius, team) = (o.pos, o.radius, o.team);
        let radius = rules::units(rules::AGGRO_CALL_RADIUS);
        for id in self.units.ids() {
            let Some(u) = self.units.get(id) else {
                continue;
            };
            if u.team == team || u.engage != Some(orderer) {
                continue;
            }
            // A creep hears the call within the aggro radius; a tower within
            // its own reach, the same way it was called on.
            let heard = if u.is_creep() {
                u.pos.within(around, radius)
            } else if u.kind == UnitKind::Tower {
                u.pos
                    .within(around, u.attack_range + u.radius + orderer_radius)
            } else {
                false
            };
            if !heard {
                continue;
            }
            let u = self.units.get_mut(id).expect("iterating live ids");
            u.engage = None;
            u.provoked_ticks = 0;
            u.aggro_cooldown = rules::AGGRO_CALL_COOLDOWN_TICKS;
            u.shunned = Some(orderer);
        }
    }

    /// Towers, creeps and idle heroes choose what to attack.
    fn aggro(&mut self) {
        for id in self.units.ids() {
            let Some(unit) = self.units.get(id) else {
                continue;
            };
            // A dead or vanished engagement is dropped first.
            if let Some(t) = unit.engage
                && self.units.get(t).is_none_or(|u| u.hp <= 0)
            {
                self.units.get_mut(id).expect("iterating live ids").engage = None;
            }
            let Some(unit) = self.units.get(id) else {
                continue;
            };
            if !unit.can_attack() {
                continue;
            }
            let acquisition = rules::units(rules::ACQUISITION_RANGE);
            match unit.kind {
                UnitKind::Tower | UnitKind::Fountain => {
                    // A building keeps its target while it stays in reach,
                    // whether it took it as the closest or was called onto it.
                    if unit.engage.is_none() || !self.engagement_in_range(id) {
                        let pick = self.pick_target(id, None);
                        self.units.get_mut(id).expect("iterating live ids").engage = pick;
                    }
                }
                UnitKind::CreepNeutral | UnitKind::Roshan => {
                    let camp = unit.camp;
                    if unit.returning {
                        // Deaf on the way home; arriving heals in full.
                        if unit.pos.within(camp, rules::units(rules::NEUTRAL_RETURN)) {
                            let u = self.units.get_mut(id).expect("iterating live ids");
                            u.returning = false;
                            u.order = UnitOrder::Idle;
                            u.hp = u.max_hp;
                        }
                        continue;
                    }
                    let leash = rules::units(rules::NEUTRAL_LEASH);
                    let target = unit.engage.and_then(|t| self.units.get(t));
                    if let Some(t) = target {
                        if !t.pos.within(camp, leash) || !unit.pos.within(camp, leash) {
                            // Dragged too far: give up and walk home.
                            let u = self.units.get_mut(id).expect("iterating live ids");
                            u.engage = None;
                            u.returning = true;
                            u.order = UnitOrder::Move { pos: camp };
                        }
                    } else {
                        let pick =
                            self.pick_target(id, Some(rules::units(rules::NEUTRAL_AGGRO_RANGE)));
                        self.units.get_mut(id).expect("iterating live ids").engage = pick;
                    }
                }
                UnitKind::CreepMelee | UnitKind::CreepRanged | UnitKind::CreepSiege => {
                    // A non-hero target is held until it dies or the chase
                    // breaks: standing closer steals no attention. A hero
                    // holds attention only for the aggro window, however it
                    // was gained. Past the leash a calm creep goes deaf and
                    // walks home; an open window overrides the leash.
                    let mut engage = unit.engage;
                    let mut returning = unit.returning;
                    let mut window = unit.provoked_ticks;
                    if window > 0 {
                        returning = false;
                        if engage.is_none() {
                            engage = self.pick_target(
                                id,
                                Some(rules::units(rules::CREEP_ACQUISITION_RANGE)),
                            );
                        }
                    } else {
                        let off = crate::sim::lane_offset_squared(unit.lane, unit.pos);
                        let leash = i64::from(rules::units(rules::LANE_LEASH).raw);
                        let home = i64::from(rules::units(rules::LANE_RETURN).raw);
                        if returning {
                            if off <= home * home {
                                returning = false;
                            }
                        } else if off > leash * leash {
                            returning = true;
                        }
                        if returning {
                            engage = None;
                        } else {
                            // A target inside the creep's own attack range is
                            // kept, hero or not: a ranged creep keeps firing
                            // at a hero who stays in its range. Chasing a
                            // hero is what the window limits, and the window
                            // is over here.
                            let keep = engage.is_some_and(|t| {
                                self.units.get(t).is_some_and(|tu| {
                                    if tu.kind != UnitKind::Hero {
                                        unit.pos
                                            .within(tu.pos, rules::units(rules::CREEP_CHASE_RANGE))
                                    } else {
                                        in_attack_range(unit, tu, Fixed::ZERO)
                                    }
                                })
                            });
                            if !keep {
                                // A hero's window ended: re-assess from
                                // scratch. An adjacent hero is simply the
                                // closest again; a kited chase loses to
                                // whatever got closer on the way.
                                engage = self.pick_target(
                                    id,
                                    Some(rules::units(rules::CREEP_ACQUISITION_RANGE)),
                                );
                                // A freshly acquired hero opens an aggro window.
                                if engage
                                    .and_then(|t| self.units.get(t))
                                    .is_some_and(|tu| tu.kind == UnitKind::Hero)
                                {
                                    window = rules::CREEP_PROVOKE_TICKS;
                                }
                            }
                        }
                    }
                    let unit = self.units.get_mut(id).expect("iterating live ids");
                    unit.engage = engage;
                    unit.returning = returning;
                    unit.provoked_ticks = window;
                    if engage.is_none() {
                        // Home is the nearest point of its own lane; duty is
                        // marching that lane's waypoints to the enemy Ancient.
                        let push_to = if returning {
                            crate::sim::lane_return_point(unit.lane, unit.pos)
                        } else {
                            let route = crate::sim::lane_route(unit.team, unit.lane);
                            let mut at = usize::from(unit.lane_step).min(route.len() - 1);
                            if at + 1 < route.len()
                                && unit
                                    .pos
                                    .within(route[at], rules::units(rules::LANE_WAYPOINT_RADIUS))
                            {
                                at += 1;
                            }
                            unit.lane_step = at as u8;
                            route[at]
                        };
                        unit.order = UnitOrder::AttackMove { pos: push_to };
                    }
                }
                UnitKind::Hero => match unit.order {
                    UnitOrder::Attack { target, .. } => {
                        // An enemy is attacked; an ally is denied when low
                        // enough and merely followed until then.
                        let engage = self.units.get(target).and_then(|t| {
                            if t.team != unit.team || deniable(t) {
                                Some(target)
                            } else {
                                None
                            }
                        });
                        self.units.get_mut(id).expect("iterating live ids").engage = engage;
                    }
                    UnitOrder::Hold => {
                        if unit.engage.is_none() {
                            let pick = self.pick_target(id, None);
                            self.units.get_mut(id).expect("iterating live ids").engage = pick;
                        }
                    }
                    UnitOrder::AttackMove { .. } => {
                        if unit.engage.is_none() {
                            let pick = self.pick_target(id, Some(acquisition));
                            self.units.get_mut(id).expect("iterating live ids").engage = pick;
                        }
                    }
                    // An idle hero starts no fight of its own: arriving
                    // somewhere or stopping leaves the wave alone. A fight
                    // continues only from an attack order, handled above.
                    UnitOrder::Idle | UnitOrder::Move { .. } => {}
                },
                _ => {}
            }
        }
    }

    /// Whether a unit's current engagement is inside its own attack range.
    fn engagement_in_range(&self, id: EntityId) -> bool {
        let Some(unit) = self.units.get(id) else {
            return false;
        };
        let Some(target) = unit.engage.and_then(|t| self.units.get(t)) else {
            return false;
        };
        in_attack_range(unit, target, Fixed::ZERO)
    }

    /// The nearest attackable enemy, by a fixed priority.
    ///
    /// `range` limits the search; absent means the unit's own attack range.
    /// Buildings prefer creeps over heroes, which is what `creeps_first` says.
    /// The closest attackable enemy within reach.
    ///
    /// `range` limits the search; absent means the unit's own attack range.
    /// Distance is the whole priority. A shunned unit is taken only when
    /// nobody else is in reach: a call-off redirects if possible.
    fn pick_target(&self, id: EntityId, range: Option<Fixed>) -> Option<EntityId> {
        let unit = self.units.get(id)?;
        let mut best: Option<(i64, EntityId)> = None;
        let mut best_shunned: Option<(i64, EntityId)> = None;
        for (other_id, other) in self.units.iter() {
            if other.team == unit.team || other.invulnerable || other.hp <= 0 {
                continue;
            }
            let reach = match range {
                Some(r) => r + unit.radius + other.radius,
                None => unit.attack_range + unit.radius + other.radius,
            };
            if !unit.pos.within(other.pos, reach) {
                continue;
            }
            let key = (unit.pos.distance_squared(other.pos), other_id);
            let slot = if unit.aggro_cooldown > 0 && unit.shunned == Some(other_id) {
                &mut best_shunned
            } else {
                &mut best
            };
            if slot.is_none_or(|b| key < b) {
                *slot = Some(key);
            }
        }
        best.or(best_shunned).map(|(_, id)| id)
    }

    /// Turns a unit and, when `walk`, takes one step along its route.
    ///
    /// Routes are planned against structures only; other units are met at
    /// contact. A step into a unit that stands this tick becomes a sidestep
    /// tracing around its circle at once. A step into a fellow walker is a
    /// full stop: the unit presses on the spot for the block wait and only
    /// then starts sidestepping around, so a body kept moving across the
    /// route stops it over and over.
    fn advance_unit(&mut self, id: EntityId, dest: Vec2, walk: bool) {
        let Some(unit) = self.units.get(id) else {
            return;
        };
        let pos = unit.pos;
        if !walk {
            let desired = facing_towards(pos, dest);
            let facing = if unit.is_structure() {
                desired
            } else {
                turn_towards(unit.facing, desired, rules::TURN_RATE_BRADS)
            };
            let u = self.units.get_mut(id).expect("looked up above");
            u.facing = facing;
            u.stuck_ticks = 0;
            return;
        }
        // A route in hand is walked out; otherwise the unit goes straight or
        // plans around structures.
        let mut path = unit.path.clone();
        if !unit
            .path_goal
            .within(dest, rules::units(rules::REPATH_DRIFT))
        {
            path.clear();
        }
        while path
            .first()
            .is_some_and(|w| pos.within(*w, rules::units(rules::WAYPOINT_RADIUS)))
        {
            path.remove(0);
        }
        if path.is_empty() && !grid_los(&self.grid, pos, dest) {
            path = find_path(&self.grid, pos, dest);
            while path
                .first()
                .is_some_and(|w| pos.within(*w, rules::units(rules::WAYPOINT_RADIUS)))
            {
                path.remove(0);
            }
        }
        let waypoint = path.first().copied().unwrap_or(dest);
        // The short path: aim past standing bodies instead of at them.
        let aim = steer_target(&self.units, id, pos, waypoint);
        let step = per_tick(unit.move_speed);
        let desired = facing_towards(pos, aim);
        let facing = turn_towards(unit.facing, desired, rules::TURN_RATE_BRADS);
        let mut next = pos;
        let mut stuck = unit.stuck_ticks;
        if facing_gap(facing, desired) <= rules::TURN_TOLERANCE_BRADS {
            let ahead = clamp_to_map(move_towards(pos, aim, step));
            // The static grid is a hard wall: a step into a closed cell is
            // refused outright, though a step out of one is always allowed.
            let grid_ok = self.grid.walkable(ahead) || !self.grid.walkable(pos);
            if !grid_ok {
                // Wedged against a tree or a building: the walk ends here.
            } else if !blocked_by_units(&self.units, id, pos, ahead) {
                next = ahead;
                stuck = 0;
            } else if blocked_by_stander(&self.units, id, pos, ahead) {
                // Right against a standing body: trace along its circle.
                if let Some(slide) = unit_slide(&self.units, &self.grid, id, aim, step) {
                    next = slide;
                }
            } else if stuck >= rules::BLOCK_WAIT_TICKS {
                // Pressed into a walking body long enough: seep around it.
                if let Some(slide) = unit_slide(&self.units, &self.grid, id, aim, step) {
                    next = slide;
                }
            } else {
                // A walking body ahead: try to walk through it and stop.
                stuck += 1;
            }
        }
        let u = self.units.get_mut(id).expect("looked up above");
        u.facing = facing;
        u.pos = next;
        u.stuck_ticks = stuck;
        u.path = path;
        u.path_goal = dest;
    }

    /// Everything that walks turns and takes its step. Whoever intends to
    /// stand is traced around at contact; whoever intends to walk stops
    /// whoever runs into it for the block wait.
    fn execute_movement(&mut self) {
        // What does each unit do this tick? Walking is an intent, not the
        // last tick's displacement: a jammed walker stays intangible, so a
        // pile-up never freezes into a wall of mutual blockers.
        enum Act {
            Stand,
            Face(Vec2),
            Walk(Vec2),
        }
        let mut acts: Vec<(EntityId, Act)> = Vec::new();
        for (id, unit) in self.units.iter() {
            let act =
                if unit.move_speed == Fixed::ZERO || unit.windup.is_some() || unit.recovering > 0 {
                    Act::Stand
                } else if let Some(target_id) = unit.engage {
                    match self.units.get(target_id) {
                        None => Act::Stand,
                        Some(target) if in_attack_range(unit, target, Fixed::ZERO) => {
                            Act::Face(target.pos)
                        }
                        Some(_) if unit.order == UnitOrder::Hold => Act::Stand,
                        Some(target) => Act::Walk(target.pos),
                    }
                } else {
                    match unit.order {
                        UnitOrder::Move { pos } | UnitOrder::AttackMove { pos } => {
                            if unit.pos == pos {
                                Act::Stand
                            } else {
                                Act::Walk(pos)
                            }
                        }
                        // An attack order without an engagement is an ally being
                        // followed: walk up and wait for something to deny.
                        UnitOrder::Attack { last_seen, .. } => {
                            if unit
                                .pos
                                .within(last_seen, rules::units(rules::FOLLOW_DISTANCE))
                            {
                                Act::Stand
                            } else {
                                Act::Walk(last_seen)
                            }
                        }
                        UnitOrder::Idle | UnitOrder::Hold => Act::Stand,
                    }
                };
            acts.push((id, act));
        }
        for (id, act) in &acts {
            let walks = matches!(act, Act::Walk(_));
            self.units
                .get_mut(*id)
                .expect("collected from live ids")
                .moving = walks;
        }
        for (id, act) in acts {
            match act {
                Act::Stand => {
                    let u = self.units.get_mut(id).expect("collected from live ids");
                    u.stuck_ticks = 0;
                    if let UnitOrder::Move { pos } | UnitOrder::AttackMove { pos } = u.order
                        && u.pos == pos
                    {
                        u.order = UnitOrder::Idle;
                    }
                }
                Act::Face(at) => self.advance_unit(id, at, false),
                Act::Walk(dest) => {
                    self.advance_unit(id, dest, true);
                    let u = self.units.get_mut(id).expect("collected from live ids");
                    if let UnitOrder::Move { pos } | UnitOrder::AttackMove { pos } = u.order
                        && u.pos == pos
                    {
                        u.order = UnitOrder::Idle;
                    }
                }
            }
        }
    }
}
