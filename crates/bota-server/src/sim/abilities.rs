//! The ability engine and the ability implementations.

use bota_proto::{
    AbilityId, AbilitySlot, DamageKind, EntityId, EventKind, OrderTarget, RejectReason, Vec2,
};

use crate::sim::{Event, Projectile, World, rules};

/// One ability slot of a hero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbilityState {
    /// Which ability sits here.
    pub id: AbilityId,
    /// Current level. Zero until learned.
    pub level: u8,
    /// Ticks until it can be cast again.
    pub cooldown: u32,
}

/// A cast accepted this tick, waiting for the ability phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingCast {
    /// Which slot is being cast.
    pub slot: AbilitySlot,
    /// What it is aimed at.
    pub target: OrderTarget,
}

/// What a cast must be aimed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastTarget {
    /// A passive. Never cast.
    Passive,
    /// Cast with no target.
    NoTarget,
    /// Cast at an enemy unit.
    EnemyUnit,
}

/// The Sylla kit. Every hero carries it until more heroes arrive.
pub fn hero_kit() -> Vec<AbilityState> {
    (0..4)
        .map(|slot| AbilityState {
            id: AbilityId(slot),
            level: 0,
            cooldown: 0,
        })
        .collect()
}

/// What an ability must be aimed at.
pub fn cast_target(id: AbilityId) -> CastTarget {
    match id.0 {
        0 => CastTarget::Passive,
        1 | 3 => CastTarget::NoTarget,
        2 => CastTarget::EnemyUnit,
        _ => CastTarget::Passive,
    }
}

/// The mana a cast costs at a level. Zero for passives and level zero.
pub fn mana_cost(id: AbilityId, level: u8) -> i32 {
    if level == 0 {
        return 0;
    }
    let l = usize::from(level - 1);
    match id.0 {
        1 => rules::SYLLA_FRENZY_MANA[l],
        2 => rules::SYLLA_BOUNCE_MANA[l],
        3 => rules::SYLLA_MULTI_MANA[l],
        _ => 0,
    }
}

/// The cooldown a cast starts at a level, in ticks.
pub fn cast_cooldown(id: AbilityId, level: u8) -> u32 {
    if level == 0 {
        return 0;
    }
    let l = usize::from(level - 1);
    match id.0 {
        1 => rules::SYLLA_FRENZY_COOLDOWN[l],
        2 => rules::SYLLA_BOUNCE_COOLDOWN[l],
        3 => rules::SYLLA_MULTI_COOLDOWN[l],
        _ => 0,
    }
}

/// The level cap of a slot.
pub fn slot_cap(slot: AbilitySlot) -> u8 {
    if slot.0 == 3 {
        rules::ULT_MAX_LEVEL
    } else {
        rules::ABILITY_MAX_LEVEL
    }
}

/// The hero level a slot's next ability level requires.
///
/// Basic levels come at hero levels 1, 3, 5, 7; ultimate levels follow
/// [`rules::ULT_LEVEL_FLOORS`].
pub fn required_hero_level(slot: AbilitySlot, next_level: u8) -> u8 {
    if slot.0 == 3 {
        rules::ULT_LEVEL_FLOORS[usize::from(next_level.min(rules::ULT_MAX_LEVEL) - 1)]
    } else {
        2 * next_level - 1
    }
}

/// Skill points a seat has not spent yet.
pub fn unspent_points(hero_level: u8, abilities: &[AbilityState]) -> u8 {
    let spent: u8 = abilities.iter().map(|a| a.level).sum();
    hero_level.saturating_sub(spent)
}

impl World {
    /// Whether a seat may spend a point on this slot right now.
    pub fn validate_level_up(
        &self,
        seat_slot: bota_proto::SlotId,
        slot: AbilitySlot,
    ) -> Result<(), RejectReason> {
        let seat = self.seat(seat_slot).expect("validated live");
        let Some(state) = seat.abilities.get(usize::from(slot.0)) else {
            return Err(RejectReason::EmptySlot);
        };
        if state.level >= slot_cap(slot)
            || unspent_points(seat.level, &seat.abilities) == 0
            || seat.level < required_hero_level(slot, state.level + 1)
        {
            return Err(RejectReason::CannotLevelUp);
        }
        Ok(())
    }

    /// Whether a seat may cast this slot at this target right now.
    pub fn validate_cast(
        &self,
        seat_slot: bota_proto::SlotId,
        unit_id: EntityId,
        slot: AbilitySlot,
        target: &OrderTarget,
    ) -> Result<(), RejectReason> {
        let seat = self.seat(seat_slot).expect("validated live");
        let team = seat.team;
        let unit = self.units.get(unit_id).expect("validated live");
        let Some(state) = seat.abilities.get(usize::from(slot.0)) else {
            return Err(RejectReason::EmptySlot);
        };
        if state.level == 0 {
            return Err(RejectReason::EmptySlot);
        }
        match cast_target(state.id) {
            CastTarget::Passive => return Err(RejectReason::WrongTargetKind),
            CastTarget::NoTarget => {
                if !matches!(target, OrderTarget::None) {
                    return Err(RejectReason::WrongTargetKind);
                }
            }
            CastTarget::EnemyUnit => {
                let OrderTarget::Unit { target } = target else {
                    return Err(RejectReason::WrongTargetKind);
                };
                if !self.can_see(team, *target) {
                    return Err(RejectReason::UnknownTarget);
                }
                let victim = self.units.get(*target).expect("can_see checked existence");
                if victim.team == team || victim.invulnerable || victim.is_structure() {
                    return Err(RejectReason::WrongTargetKind);
                }
                let reach =
                    rules::units(rules::SYLLA_BOUNCE_CAST_RANGE) + unit.radius + victim.radius;
                if !unit.pos.within(victim.pos, reach) {
                    return Err(RejectReason::OutOfRange);
                }
            }
        }
        if state.cooldown > 0 {
            return Err(RejectReason::OnCooldown);
        }
        if unit.mana < mana_cost(state.id, state.level) {
            return Err(RejectReason::NotEnoughMana);
        }
        Ok(())
    }

    /// Spends a skill point. The order was validated this tick.
    pub fn apply_level_up(&mut self, seat_slot: bota_proto::SlotId, slot: AbilitySlot) {
        if self.validate_level_up(seat_slot, slot).is_err() {
            return;
        }
        let seat = self.seat_mut(seat_slot).expect("validated live");
        seat.abilities[usize::from(slot.0)].level += 1;
    }

    /// Executes every pending cast: spends mana, starts cooldowns, applies
    /// the effect. A cast whose costs or target died since validation fizzles
    /// silently.
    pub fn run_casts(&mut self, events: &mut Vec<Event>) {
        for id in self.units.ids() {
            let Some(unit) = self.units.get(id) else {
                continue;
            };
            let Some(cast) = unit.pending_cast else {
                continue;
            };
            let state = unit
                .owner
                .and_then(|s| self.seat(s))
                .and_then(|seat| seat.abilities.get(usize::from(cast.slot.0)).cloned());
            let Some(state) = state else {
                self.units
                    .get_mut(id)
                    .expect("looked up above")
                    .pending_cast = None;
                continue;
            };
            let cost = mana_cost(state.id, state.level);
            if state.level == 0 || state.cooldown > 0 || unit.mana < cost {
                self.units
                    .get_mut(id)
                    .expect("looked up above")
                    .pending_cast = None;
                continue;
            }
            let done = match state.id.0 {
                1 => self.cast_frenzy(id, state.level),
                2 => self.cast_bounce(id, state.level, cast.target),
                3 => self.cast_multishot(id, state.level),
                _ => false,
            };
            let u = self.units.get_mut(id).expect("looked up above");
            u.pending_cast = None;
            if done {
                u.mana -= cost;
                let pos = u.pos;
                let team = u.team;
                let owner = u.owner.expect("a cast comes from a seat");
                self.seat_mut(owner).expect("owner seat is live").abilities
                    [usize::from(cast.slot.0)]
                .cooldown = cast_cooldown(state.id, state.level);
                events.push(Event {
                    kind: EventKind::AbilityCast {
                        caster: id,
                        ability: state.id,
                    },
                    visible_to: self.point_visibility(pos, team),
                });
            }
        }
    }

    fn cast_frenzy(&mut self, id: EntityId, level: u8) -> bool {
        let u = self.units.get_mut(id).expect("cast validated");
        u.frenzy_ticks = rules::SYLLA_FRENZY_TICKS;
        u.frenzy_pct = rules::SYLLA_FRENZY_HASTE_PCT[usize::from(level - 1)];
        true
    }

    fn cast_bounce(&mut self, id: EntityId, level: u8, target: OrderTarget) -> bool {
        let OrderTarget::Unit { target } = target else {
            return false;
        };
        let Some(unit) = self.units.get(id) else {
            return false;
        };
        let Some(victim) = self.units.get(target) else {
            return false;
        };
        if victim.hp <= 0
            || victim.team == unit.team
            || victim.invulnerable
            || victim.is_structure()
        {
            return false;
        }
        let l = usize::from(level - 1);
        self.projectiles.insert(Projectile {
            pos: unit.pos,
            facing: crate::sim::facing_towards(unit.pos, victim.pos),
            speed: rules::units(rules::SYLLA_BOUNCE_SPEED),
            team: unit.team,
            source: Some(id),
            slot: unit.owner,
            target,
            damage: rules::SYLLA_BOUNCE_DAMAGE[l],
            kind: DamageKind::Magical,
            ability: Some(AbilityId(2)),
            launch_tier: 0,
            crit: false,
            bounces_left: rules::SYLLA_BOUNCE_COUNT[l],
            bounced: vec![target],
        });
        true
    }

    fn cast_multishot(&mut self, id: EntityId, level: u8) -> bool {
        let Some(unit) = self.units.get(id) else {
            return false;
        };
        let (pos, team, owner, damage) = (unit.pos, unit.team, unit.owner, unit.attack_damage);
        let radius = rules::units(rules::SYLLA_MULTI_RADIUS);
        let pct = rules::SYLLA_MULTI_DMG_PCT[usize::from(level - 1)];
        let volley: Vec<(EntityId, Vec2)> = self
            .units
            .iter()
            .filter(|(tid, t)| {
                *tid != id
                    && t.team != team
                    && t.hp > 0
                    && !t.invulnerable
                    && !t.is_structure()
                    && t.pos.within(pos, radius)
            })
            .map(|(tid, t)| (tid, t.pos))
            .collect();
        if volley.is_empty() {
            return false;
        }
        for (tid, tpos) in volley {
            self.projectiles.insert(Projectile {
                pos,
                facing: crate::sim::facing_towards(pos, tpos),
                speed: rules::units(rules::HERO_PROJECTILE_SPEED),
                team,
                source: Some(id),
                slot: owner,
                target: tid,
                damage: (i64::from(damage) * i64::from(pct) / 100) as i32,
                kind: DamageKind::Physical,
                ability: Some(AbilityId(3)),
                launch_tier: 0,
                crit: false,
                bounces_left: 0,
                bounced: Vec::new(),
            });
        }
        true
    }

    /// The critical multiplier of this attack, if the attacker crits now.
    ///
    /// Rolls the attacker's hidden chance stream; the roll is spent whether
    /// or not the hit later connects.
    pub fn roll_crit(&mut self, attacker: EntityId) -> Option<i32> {
        let unit = self.units.get(attacker)?;
        let seat = self.seat(unit.owner?)?;
        let level = seat.abilities.first().filter(|a| a.level > 0)?.level;
        let l = usize::from(level - 1);
        let unit = self.units.get_mut(attacker).expect("looked up above");
        let chance = unit.crit.as_mut()?;
        if chance.roll(rules::SYLLA_CRIT_CHANCE[l]) {
            Some(rules::SYLLA_CRIT_MULT_PCT[l])
        } else {
            None
        }
    }

    /// Ticks down ability cooldowns and timed effects. Cooldowns live on the
    /// seat and keep running while the hero is dead.
    pub fn tick_ability_cooldowns(&mut self) {
        for seat in &mut self.seats {
            for a in &mut seat.abilities {
                a.cooldown = a.cooldown.saturating_sub(1);
            }
        }
        for (_, unit) in self.units.iter_mut() {
            if unit.frenzy_ticks > 0 {
                unit.frenzy_ticks -= 1;
                if unit.frenzy_ticks == 0 {
                    unit.frenzy_pct = 0;
                }
            }
        }
    }
}

/// Where the bounce jumps next: the closest enemy in bounce range that the
/// projectile has not hit yet.
pub fn next_bounce_target(
    world: &World,
    team: bota_proto::Team,
    from: Vec2,
    hit: &[EntityId],
) -> Option<EntityId> {
    let range = rules::units(rules::SYLLA_BOUNCE_RANGE);
    world
        .units
        .iter()
        .filter(|(id, u)| {
            u.team != team
                && u.hp > 0
                && !u.invulnerable
                && !u.is_structure()
                && !hit.contains(id)
                && u.pos.within(from, range)
        })
        .min_by_key(|(id, u)| (u.pos.distance_squared(from), id.idx))
        .map(|(id, _)| id)
}

/// The wire view of a seat's ability slots.
pub fn ability_views(abilities: &[AbilityState]) -> Vec<bota_proto::AbilityView> {
    abilities
        .iter()
        .map(|a| bota_proto::AbilityView {
            id: a.id,
            level: a.level,
            cooldown_left: a.cooldown,
            mana_cost: mana_cost(a.id, a.level.max(1)),
        })
        .collect()
}
