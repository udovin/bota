//! One tick of the world, in its fixed order.

use bota_proto::{EntityId, EventKind, Fixed, Order, RejectReason, SlotId, Team, UnitKind, Vec2};

use crate::sim::{
    UnitOrder, World, clamp_to_map, facing_gap, facing_towards, find_path, grid_los,
    in_attack_range, per_tick, rules, turn_towards, walk_step,
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
        crate::sim::push_apart(&mut self.units, &self.grid); // 6. bodies apart
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
                        // The order itself is the aggro check; whether it
                        // calls or calls off is read off the target there.
                        self.order_aggro(unit_id, target);
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
                    crate::sim::acquire(
                        self,
                        id,
                        rules::units(rules::ACQUISITION_RANGE),
                        crate::sim::PriorityOrder::Normal,
                    )
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
            unit.order_cooldown = unit.order_cooldown.saturating_sub(1);
        }
        self.tick_ability_cooldowns();
    }

    /// An attack order by a hero makes the other side's creeps and towers
    /// look again.
    ///
    /// The order alone does it, whether the attack happens or not. Only an
    /// order at an enemy hero calls, and only an order at an ally calls off;
    /// an order at an enemy creep is a last hit and moves nobody. A creep
    /// ranks its candidates the usual way, so the ordering hero wins among
    /// equally close ones. A tower switches outright. Each unit answers at
    /// most once per [`rules::ORDER_AGGRO_COOLDOWN_TICKS`].
    pub(crate) fn order_aggro(&mut self, orderer: EntityId, target: EntityId) {
        let Some(o) = self.units.get(orderer) else {
            return;
        };
        if o.kind != UnitKind::Hero {
            return;
        }
        let (around, orderer_radius, team) = (o.pos, o.radius, o.team);
        let Some(t) = self.units.get(target) else {
            return;
        };
        let calls_on = t.team != team && t.kind == UnitKind::Hero;
        let calls_off = t.team == team;
        if !calls_on && !calls_off {
            return;
        }
        for id in self.units.ids() {
            let Some(u) = self.units.get(id) else {
                continue;
            };
            if u.team == team || !u.can_attack() {
                continue;
            }
            if u.is_creep() {
                if u.order_cooldown > 0 || !u.pos.within(around, u.acquisition_range) {
                    continue;
                }
                // The five minute rule is about being called on. Letting go
                // is never restricted.
                if calls_on && !self.aggroable_yet(id) {
                    continue;
                }
                let range = u.acquisition_range;
                let priority = crate::sim::priority_of(u);
                // Both directions work over the ranking, not through it.
                // Called on, the creep takes the offender however close the
                // bystanders are. Called off, that hero goes last however
                // close it is: anyone else is taken first, and it is taken
                // again only when there is nobody else.
                let pick = if calls_on {
                    Some(orderer)
                } else {
                    crate::sim::acquire_demoting(self, id, range, priority, Some(orderer))
                };
                let u = self.units.get_mut(id).expect("iterating live ids");
                u.engage = pick;
                u.order_cooldown = rules::ORDER_AGGRO_COOLDOWN_TICKS;
                u.returning = false;
                if let Some(crate::sim::CreepAi::Lane(ai)) = u.ai.as_mut() {
                    ai.chase_left = rules::CREEP_CHASE_TICKS;
                    ai.provoked = if calls_on {
                        rules::ORDER_AGGRO_HOLD_TICKS
                    } else {
                        0
                    };
                }
            } else if u.kind == UnitKind::Tower {
                // A tower does not weigh the offender against anything: a
                // dive draws it outright, and a click at an ally lets go.
                // Letting go answers at once, however recently it was drawn.
                if !u
                    .pos
                    .within(around, u.attack_range + u.radius + orderer_radius)
                {
                    continue;
                }
                if calls_on && u.order_cooldown > 0 {
                    continue;
                }
                if calls_off && u.engage != Some(orderer) {
                    continue;
                }
                let u = self.units.get_mut(id).expect("iterating live ids");
                u.engage = if calls_on { Some(orderer) } else { None };
                u.order_cooldown = rules::ORDER_AGGRO_COOLDOWN_TICKS;
            }
        }
    }

    /// Whether a lane creep may be aimed by an attack order at all yet.
    ///
    /// Free from [`rules::FREE_AGGRO_TICK`]. Before it, only a creep that
    /// already has an enemy lane creep or a neutral in acquisition range, or
    /// that stands near its own tier-one tower.
    fn aggroable_yet(&self, id: EntityId) -> bool {
        if self.tick >= rules::FREE_AGGRO_TICK {
            return true;
        }
        let Some(u) = self.units.get(id) else {
            return false;
        };
        let busy = self.units.iter().any(|(_, o)| {
            (o.is_creep() && o.team != u.team || o.team == Team::Neutral)
                && o.hp > 0
                && u.pos.within(o.pos, u.acquisition_range)
        });
        if busy {
            return true;
        }
        let near_home = rules::units(rules::EARLY_AGGRO_TOWER_RANGE);
        self.units.iter().any(|(_, o)| {
            o.kind == UnitKind::Tower
                && o.tier == 1
                && o.team == u.team
                && u.pos.within(o.pos, near_home)
        })
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
                        let pick = crate::sim::acquire(
                            self,
                            id,
                            unit.attack_range,
                            crate::sim::PriorityOrder::Normal,
                        );
                        self.units.get_mut(id).expect("iterating live ids").engage = pick;
                    }
                }
                UnitKind::CreepNeutral | UnitKind::Roshan => {
                    let Some(crate::sim::CreepAi::Neutral(mut ai)) = unit.ai.clone() else {
                        continue;
                    };
                    ai.reaggro_block = ai.reaggro_block.saturating_sub(1);
                    let mut engage = unit.engage;
                    if engage.and_then(|t| self.units.get(t)).is_none() {
                        engage = None;
                    }
                    let guard = rules::units(rules::NEUTRAL_GUARD_DISTANCE);
                    if unit.pos.within(ai.home, guard) {
                        // Home ground: the window is whole again, and getting
                        // all the way back ends the walk and restores the long
                        // window for next time.
                        ai.leash_left = ai.next_window;
                        if ai.going_home
                            && unit
                                .pos
                                .within(ai.home, rules::units(rules::NEUTRAL_RETURN))
                        {
                            ai.going_home = false;
                            ai.next_window = rules::NEUTRAL_AGGRO_WINDOW;
                        }
                    } else if ai.leash_left == 0 {
                        // Dragged out and the window is spent: let go, walk
                        // back, and stay deaf to a body standing close until
                        // home, and to damage for a moment.
                        engage = None;
                        ai.going_home = true;
                        ai.reaggro_block = rules::NEUTRAL_REAGGRO_BLOCK;
                        ai.next_window = rules::NEUTRAL_SHORT_WINDOW;
                    } else {
                        ai.leash_left -= 1;
                    }
                    if engage.is_none() && !ai.going_home {
                        // Proximity is what wakes it: something hostile inside
                        // the aggro radius. What it then swings at is the
                        // ordinary ranking over its own acquisition range.
                        let woken = self.units.iter().any(|(other, o)| {
                            other != id
                                && crate::sim::hostile(unit, o)
                                && unit
                                    .pos
                                    .within(o.pos, rules::units(rules::NEUTRAL_AGGRO_RANGE))
                        });
                        if woken {
                            engage = crate::sim::acquire(
                                self,
                                id,
                                unit.acquisition_range,
                                crate::sim::PriorityOrder::Normal,
                            );
                            ai.leash_left = ai.next_window;
                        }
                    } else if engage.is_some() {
                        // Awake, and still choosing by the ordinary rules.
                        let pick = crate::sim::acquire(
                            self,
                            id,
                            unit.acquisition_range,
                            crate::sim::PriorityOrder::Normal,
                        );
                        if let Some(t) = pick {
                            engage = Some(t);
                        }
                    }
                    let unit = self.units.get_mut(id).expect("iterating live ids");
                    unit.engage = engage;
                    unit.ai = Some(crate::sim::CreepAi::Neutral(ai));
                    if engage.is_none() {
                        unit.order = if unit
                            .pos
                            .within(ai.home, rules::units(rules::NEUTRAL_RETURN))
                        {
                            UnitOrder::Idle
                        } else {
                            UnitOrder::Move { pos: ai.home }
                        };
                    }
                }
                UnitKind::CreepMelee
                | UnitKind::CreepFlagbearer
                | UnitKind::CreepRanged
                | UnitKind::CreepSiege => {
                    let Some(crate::sim::CreepAi::Lane(mut ai)) = unit.ai.clone() else {
                        continue;
                    };
                    ai.provoked = ai.provoked.saturating_sub(1);
                    let mut engage = unit.engage;
                    // The ranking runs every tick. Anything it returns is by
                    // definition inside acquisition range and at least as
                    // good as what is held, so this is both the first choice
                    // and every switch after it: off a building onto an
                    // arriving creep, off a far target onto a nearer one.
                    let priority = crate::sim::priority_of(unit);
                    let held = engage.and_then(|t| self.units.get(t));
                    let held_alive = held.is_some();
                    // A creep does not abandon what it is hitting: whatever it
                    // holds, in reach or not yet lost, is kept, so walking a
                    // hero past a busy creep steals nothing. It weighs its
                    // options again when the target leaves its reach, when a
                    // better class comes into reach, or when the target is
                    // gone. A pull silences all of that for its three seconds.
                    let in_reach = held.is_some_and(|t| in_attack_range(unit, t, Fixed::ZERO));
                    let look_again = !held_alive
                        || (ai.provoked == 0 && !in_reach)
                        || engage.is_some_and(|t| {
                            crate::sim::outranked(self, id, t, unit.attack_range, priority)
                        });
                    let pick = if look_again {
                        crate::sim::acquire(self, id, unit.acquisition_range, priority)
                    } else {
                        None
                    };
                    // An attack order handed it this target; the ranking waits
                    // its turn, but the chase below still runs down.
                    let held = ai.provoked > 0 && held_alive;
                    let took = match pick {
                        Some(t) if !held => {
                            if engage != Some(t) {
                                ai.chase_left = rules::CREEP_CHASE_TICKS;
                            }
                            engage = Some(t);
                            ai.last_seen = self.units.get(t).map(|tu| tu.pos);
                            if ai.anchor.is_none() {
                                ai.anchor = Some(unit.pos);
                            }
                            true
                        }
                        _ => false,
                    };
                    if let Some(t) = engage
                        && !took
                    {
                        match self.units.get(t) {
                            None => {
                                engage = None;
                                ai.last_seen = None;
                            }
                            Some(tu) => {
                                if !self.can_see_point(unit.team, tu.pos) {
                                    // The fog took it. The last sighting is
                                    // where the creep walks.
                                    engage = None;
                                } else {
                                    ai.last_seen = Some(tu.pos);
                                    let reach = unit.acquisition_range + unit.radius + tu.radius;
                                    if unit.pos.within(tu.pos, reach) {
                                        // Still in reach: the chase is whole
                                        // again. Only leaving the range spends
                                        // it.
                                        ai.chase_left = rules::CREEP_CHASE_TICKS;
                                    } else if ai.chase_left == 0 {
                                        engage = None;
                                        ai.last_seen = None;
                                    } else {
                                        ai.chase_left -= 1;
                                    }
                                }
                            }
                        }
                    }
                    let route = &crate::sim::lane_routes(self.map)
                        [crate::sim::team_index(unit.team)][usize::from(unit.lane)];
                    let waypoint_radius = rules::units(rules::LANE_WAYPOINT_RADIUS);
                    let on_lane = crate::sim::lane_offset_squared(self.map, unit.lane, unit.pos)
                        <= waypoint_radius.squared_raw();
                    let push_to = if engage.is_some() {
                        None
                    } else if let Some(spot) = ai.last_seen {
                        if unit.pos.within(spot, waypoint_radius) {
                            ai.last_seen = None;
                            None
                        } else {
                            Some(spot)
                        }
                    } else if let Some(anchor) = ai.anchor {
                        // Back on the lane, or back at the spot it left:
                        // either way there is nothing to walk back to.
                        if on_lane || unit.pos.within(anchor, waypoint_radius) {
                            ai.anchor = None;
                            None
                        } else {
                            Some(anchor)
                        }
                    } else {
                        None
                    };
                    let push_to = push_to.unwrap_or_else(|| {
                        // Every waypoint already inside the radius is
                        // cleared at once, not one a tick.
                        let at =
                            crate::sim::advance_waypoint(route, usize::from(ai.step), unit.pos);
                        ai.step = at as u16;
                        route[at]
                    });
                    let unit = self.units.get_mut(id).expect("iterating live ids");
                    unit.engage = engage;
                    unit.ai = Some(crate::sim::CreepAi::Lane(ai));
                    if engage.is_none() {
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
                            let pick = crate::sim::acquire(
                                self,
                                id,
                                unit.attack_range,
                                crate::sim::PriorityOrder::Normal,
                            );
                            self.units.get_mut(id).expect("iterating live ids").engage = pick;
                        }
                    }
                    UnitOrder::AttackMove { .. } => {
                        if unit.engage.is_none() {
                            let pick = crate::sim::acquire(
                                self,
                                id,
                                acquisition,
                                crate::sim::PriorityOrder::Normal,
                            );
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
                turn_towards(unit.facing, desired, unit.turn_rate)
            };
            let u = self.units.get_mut(id).expect("looked up above");
            u.facing = facing;
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
        let step = per_tick(unit.move_speed);
        // A creep marches; anything a player drives walks a short path of its
        // own. The two never share a step.
        let held = match &unit.ai {
            Some(crate::sim::CreepAi::Lane(ai)) => ai.trace,
            _ => None,
        };
        let shoving = unit.shove >= rules::MARCH_SHOVE_TICKS;
        let (aim, trace) = if unit.is_creep() || unit.team == Team::Neutral {
            crate::sim::march_aim(&self.units, &self.grid, id, waypoint, step, held, shoving)
        } else {
            (waypoint, None)
        };
        let desired = facing_towards(pos, aim);
        let facing = turn_towards(unit.facing, desired, unit.turn_rate);
        let mut next = pos;
        // Turning comes first and costs the tick: a creep that has to swing
        // round a body stands still until it faces the way out.
        if facing_gap(facing, desired) <= rules::TURN_TOLERANCE_BRADS {
            next = if unit.is_creep() || unit.team == Team::Neutral {
                crate::sim::march_step(&self.units, &self.grid, id, aim, step, shoving)
            } else {
                walk_step(&self.units, &self.grid, id, aim, step)
            };
        }
        let u = self.units.get_mut(id).expect("looked up above");
        u.facing = facing;
        // The count falls back a tick at a time rather than clearing, so a
        // creep jittering in place still reaches the point of shoving.
        u.shove = if next == pos {
            u.shove.saturating_add(1)
        } else {
            u.shove.saturating_sub(1)
        };
        u.pos = next;
        u.path = path;
        u.path_goal = dest;
        if let Some(crate::sim::CreepAi::Lane(ai)) = u.ai.as_mut() {
            ai.trace = trace;
        }
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
            // Mid-swing and recovering, a unit does not walk but still comes
            // round to what it is hitting.
            let act = if let Some(w) = unit.windup {
                self.units
                    .get(w.target)
                    .map_or(Act::Stand, |t| Act::Face(t.pos))
            } else if unit.recovering > 0 {
                unit.engage
                    .and_then(|t| self.units.get(t))
                    .map_or(Act::Stand, |t| Act::Face(t.pos))
            } else if let Some(target_id) = unit.engage {
                match self.units.get(target_id) {
                    None => Act::Stand,
                    // A tower cannot walk but still comes round to its
                    // target, so facing is decided before speed is.
                    Some(target) if in_attack_range(unit, target, Fixed::ZERO) => {
                        Act::Face(target.pos)
                    }
                    Some(_) if unit.move_speed == Fixed::ZERO || unit.order == UnitOrder::Hold => {
                        Act::Stand
                    }
                    Some(target) => Act::Walk(target.pos),
                }
            } else if unit.move_speed == Fixed::ZERO {
                Act::Stand
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
        for (id, act) in acts {
            match act {
                Act::Stand => {
                    let u = self.units.get_mut(id).expect("collected from live ids");
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
