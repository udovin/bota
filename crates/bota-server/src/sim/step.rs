//! One tick of the world, in its fixed order.

use bota_proto::{EntityId, EventKind, Fixed, Order, RejectReason, SlotId, Team, UnitKind, Vec2};

use crate::sim::{
    UnitOrder, World, clamp_to_map, facing_towards, in_attack_range, move_towards, per_tick, rules,
    separate_collisions,
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
                if target_unit.team == seat.team {
                    let deniable = target_unit.is_creep()
                        && i64::from(target_unit.hp) * 100
                            < i64::from(target_unit.max_hp) * i64::from(rules::DENY_HP_PCT);
                    if !deniable {
                        return Err(RejectReason::CannotDeny);
                    }
                }
                Ok(())
            }
            Order::CastAbility { .. } => Err(RejectReason::EmptySlot),
            Order::UseItem { .. } | Order::SellItem { .. } => Err(RejectReason::EmptySlot),
            Order::LevelUpAbility { .. } => Err(RejectReason::CannotLevelUp),
            Order::BuyItem { .. } => {
                let hero = self.units.get(unit_id).expect("seat.unit is live");
                let at_shop = hero.pos.within(
                    crate::sim::fountain_pos(seat.team),
                    rules::units(rules::FOUNTAIN_HEAL_RADIUS),
                );
                if at_shop {
                    Err(RejectReason::UnknownItem)
                } else {
                    Err(RejectReason::NotAtShop)
                }
            }
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
        self.tick_respawns(); //                                3. scheduled
        self.passive_gold(); //                                 3. scheduled
        self.regen(); //                                        4. statuses
        self.tick_cooldowns(); //                               4. statuses
        self.aggro(); //                                        5. target choice
        self.execute_movement(); //                             6. movement
        let mut dmg = Vec::new();
        self.run_attacks(&mut dmg); //                          7. attacks
        self.move_projectiles(&mut dmg); //                     7. projectiles
        //                                                      8. abilities: none yet
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
                    let last_seen = self.units.get(target).map(|t| t.pos);
                    if let Some(last_seen) = last_seen {
                        let unit = self
                            .units
                            .get_mut(unit_id)
                            .expect("borrow released and id unchanged");
                        unit.order = UnitOrder::Attack { target, last_seen };
                        unit.engage = Some(target);
                    }
                }
                // Validation rejected everything below before it got here.
                Order::CastAbility { .. }
                | Order::UseItem { .. }
                | Order::LevelUpAbility { .. }
                | Order::BuyItem { .. }
                | Order::SellItem { .. } => {}
            }
        }
    }

    /// A unit never acts on what its team cannot see: an attack order whose
    /// target fell into fog degrades to attack-moving at the last seen spot.
    fn degrade_fogged_orders(&mut self) {
        for id in self.units.ids() {
            let Some(unit) = self.units.get(id) else {
                continue;
            };
            let UnitOrder::Attack { target, last_seen } = unit.order else {
                continue;
            };
            let team = unit.team;
            let visible = self
                .units
                .get(target)
                .is_some_and(|t| t.hp > 0 && (t.team == team || self.can_see_point(team, t.pos)));
            if visible {
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
                    if unit.engage.is_none() || !self.engagement_in_range(id) {
                        let pick = self.pick_target(id, None, true);
                        self.units.get_mut(id).expect("iterating live ids").engage = pick;
                    }
                }
                UnitKind::CreepMelee | UnitKind::CreepRanged | UnitKind::CreepSiege => {
                    if unit.engage.is_none() {
                        let pick = self.pick_target(id, Some(acquisition), false);
                        let unit = self.units.get_mut(id).expect("iterating live ids");
                        unit.engage = pick;
                        if pick.is_none() {
                            let push_to = crate::sim::ancient_pos(crate::sim::enemy_of(unit.team));
                            unit.order = UnitOrder::AttackMove { pos: push_to };
                        }
                    }
                }
                UnitKind::Hero => match unit.order {
                    UnitOrder::Attack { target, .. } => {
                        let unit = self.units.get_mut(id).expect("iterating live ids");
                        if unit.engage.is_none() {
                            unit.engage = Some(target);
                        }
                    }
                    UnitOrder::Hold => {
                        if unit.engage.is_none() {
                            let pick = self.pick_target(id, None, false);
                            self.units.get_mut(id).expect("iterating live ids").engage = pick;
                        }
                    }
                    UnitOrder::Idle | UnitOrder::AttackMove { .. } => {
                        if unit.engage.is_none() {
                            let pick = self.pick_target(id, Some(acquisition), false);
                            self.units.get_mut(id).expect("iterating live ids").engage = pick;
                        }
                    }
                    UnitOrder::Move { .. } => {}
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
    fn pick_target(
        &self,
        id: EntityId,
        range: Option<Fixed>,
        creeps_first: bool,
    ) -> Option<EntityId> {
        let unit = self.units.get(id)?;
        let mut best: Option<(u8, i64, EntityId)> = None;
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
            let class = if creeps_first && other.kind == UnitKind::Hero {
                1
            } else {
                0
            };
            let key = (class, unit.pos.distance_squared(other.pos), other_id);
            if best.is_none_or(|b| key < (b.0, b.1, b.2)) {
                best = Some(key);
            }
        }
        best.map(|(_, _, id)| id)
    }

    /// Everything that walks takes its step.
    fn execute_movement(&mut self) {
        for id in self.units.ids() {
            let Some(unit) = self.units.get(id) else {
                continue;
            };
            if unit.move_speed == Fixed::ZERO || unit.windup.is_some() {
                continue;
            }
            let step = per_tick(unit.move_speed);
            if let Some(target_id) = unit.engage {
                let Some(target) = self.units.get(target_id) else {
                    continue;
                };
                if in_attack_range(unit, target, Fixed::ZERO) {
                    let facing = facing_towards(unit.pos, target.pos);
                    self.units.get_mut(id).expect("iterating live ids").facing = facing;
                } else if unit.order != UnitOrder::Hold {
                    let next = clamp_to_map(move_towards(unit.pos, target.pos, step));
                    let facing = facing_towards(unit.pos, target.pos);
                    let u = self.units.get_mut(id).expect("iterating live ids");
                    u.pos = next;
                    u.facing = facing;
                }
                continue;
            }
            match unit.order {
                UnitOrder::Move { pos } | UnitOrder::AttackMove { pos } => {
                    if unit.pos == pos {
                        self.units.get_mut(id).expect("iterating live ids").order = UnitOrder::Idle;
                        continue;
                    }
                    let next = clamp_to_map(move_towards(unit.pos, pos, step));
                    let facing = facing_towards(unit.pos, pos);
                    let u = self.units.get_mut(id).expect("iterating live ids");
                    u.pos = next;
                    u.facing = facing;
                    if u.pos == pos {
                        u.order = UnitOrder::Idle;
                    }
                }
                UnitOrder::Idle | UnitOrder::Hold | UnitOrder::Attack { .. } => {}
            }
        }
        separate_collisions(&mut self.units);
    }
}
