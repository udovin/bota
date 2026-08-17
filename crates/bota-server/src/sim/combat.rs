//! Attacks, projectiles and the damage queue.

use bota_proto::{AbilityId, Angle, DamageKind, EntityId, EventKind, Fixed, SlotId, Team, Vec2};

use crate::sim::{Event, Unit, Windup, World, facing_towards, move_towards, per_tick, rules};

/// An attack projectile in flight.
///
/// Homes onto its target and despawns if the target is gone before it lands.
#[derive(Clone, Debug)]
pub struct Projectile {
    /// Current position.
    pub pos: Vec2,
    /// Direction of travel, updated as it homes.
    pub facing: Angle,
    /// Speed in world units per second.
    pub speed: Fixed,
    /// Which side launched it.
    pub team: Team,
    /// The unit that launched it, while that unit lives.
    pub source: Option<EntityId>,
    /// The seat behind the launcher, for kill attribution.
    pub slot: Option<SlotId>,
    /// Who it is flying at.
    pub target: EntityId,
    /// Damage on impact, before the target's armor.
    pub damage: i32,
    /// Which reduction applies on impact.
    pub kind: DamageKind,
    /// Which ability launched it. Absent for a plain attack.
    pub ability: Option<AbilityId>,
}

/// One chunk of damage waiting to be applied.
#[derive(Clone, Debug)]
pub struct DamageInst {
    /// Who dealt it.
    pub source: Option<EntityId>,
    /// The seat behind the damage, for attribution.
    pub slot: Option<SlotId>,
    /// The side the damage came from.
    pub team: Team,
    /// Who takes it.
    pub target: EntityId,
    /// The amount before reductions.
    pub amount: i32,
    /// Which reduction applies.
    pub kind: DamageKind,
}

/// A unit brought to zero health this tick, waiting for death processing.
#[derive(Clone, Debug)]
pub struct Death {
    /// The dying unit.
    pub id: EntityId,
    /// The unit that dealt the killing blow, if it still exists.
    pub killer_unit: Option<EntityId>,
    /// The seat behind the killing blow.
    pub killer_slot: Option<SlotId>,
    /// The side the killing blow came from.
    pub killer_team: Team,
}

/// Whether `a` can hit `b` right now, with `extra` slack past attack range.
///
/// Range is measured edge to edge, so both radii count.
pub fn in_attack_range(a: &Unit, b: &Unit, extra: Fixed) -> bool {
    a.pos
        .within(b.pos, a.attack_range + a.radius + b.radius + extra)
}

impl World {
    /// Progresses windups, connects hits, starts new attacks.
    pub fn run_attacks(&mut self, dmg: &mut Vec<DamageInst>) {
        let leeway = rules::units(rules::ATTACK_RANGE_LEEWAY);
        for id in self.units.ids() {
            let Some(unit) = self.units.get(id) else {
                continue;
            };
            // A windup whose target died winds down to nothing.
            if let Some(w) = unit.windup {
                let target_alive = self.units.get(w.target).is_some_and(|t| t.hp > 0);
                if !target_alive {
                    self.units.get_mut(id).expect("looked up above").windup = None;
                }
            }
            let Some(unit) = self.units.get(id) else {
                continue;
            };
            if let Some(w) = unit.windup {
                // The attack connects when the windup runs out.
                if w.ticks_left > 1 {
                    let u = self.units.get_mut(id).expect("looked up above");
                    u.windup = Some(Windup {
                        target: w.target,
                        ticks_left: w.ticks_left - 1,
                    });
                } else {
                    let target = self.units.get(w.target).expect("liveness checked above");
                    let connects = in_attack_range(unit, target, leeway);
                    let (team, slot, damage, speed, from, facing) = (
                        unit.team,
                        unit.owner,
                        unit.attack_damage,
                        unit.projectile_speed,
                        unit.pos,
                        facing_towards(unit.pos, target.pos),
                    );
                    if connects {
                        match speed {
                            None => dmg.push(DamageInst {
                                source: Some(id),
                                slot,
                                team,
                                target: w.target,
                                amount: damage,
                                kind: DamageKind::Physical,
                            }),
                            Some(speed) => {
                                self.projectiles.insert(Projectile {
                                    pos: from,
                                    facing,
                                    speed,
                                    team,
                                    source: Some(id),
                                    slot,
                                    target: w.target,
                                    damage,
                                    kind: DamageKind::Physical,
                                    ability: None,
                                });
                            }
                        }
                    }
                    let u = self.units.get_mut(id).expect("looked up above");
                    u.windup = None;
                    u.facing = facing;
                }
                continue;
            }
            // No windup in progress: start one when a target is in reach.
            let Some(target_id) = unit.engage else {
                continue;
            };
            if !unit.can_attack() || unit.attack_cooldown > 0 {
                continue;
            }
            let Some(target) = self.units.get(target_id) else {
                continue;
            };
            if target.hp <= 0 || !in_attack_range(unit, target, Fixed::ZERO) {
                continue;
            }
            let facing = facing_towards(unit.pos, target.pos);
            let (point, interval) = (unit.attack_point, unit.attack_interval);
            let u = self.units.get_mut(id).expect("looked up above");
            u.windup = Some(Windup {
                target: target_id,
                ticks_left: point,
            });
            u.attack_cooldown = interval;
            u.facing = facing;
        }
    }

    /// Moves every projectile, landing hits and dropping the pointless.
    pub fn move_projectiles(&mut self, dmg: &mut Vec<DamageInst>) {
        for id in self.projectiles.ids() {
            let Some(p) = self.projectiles.get(id) else {
                continue;
            };
            let Some(target) = self.units.get(p.target) else {
                self.projectiles.remove(id);
                continue;
            };
            if target.hp <= 0 {
                self.projectiles.remove(id);
                continue;
            }
            let step = per_tick(p.speed);
            let next = move_towards(p.pos, target.pos, step);
            if next.within(target.pos, target.radius) {
                dmg.push(DamageInst {
                    source: p.source,
                    slot: p.slot,
                    team: p.team,
                    target: p.target,
                    amount: p.damage,
                    kind: p.kind,
                });
                self.projectiles.remove(id);
            } else {
                let facing = facing_towards(next, target.pos);
                let p = self.projectiles.get_mut(id).expect("looked up above");
                p.pos = next;
                p.facing = facing;
            }
        }
    }

    /// Applies queued damage, in queue order. Returns who died of it.
    pub fn resolve_damage(&mut self, dmg: Vec<DamageInst>, events: &mut Vec<Event>) -> Vec<Death> {
        let mut deaths = Vec::new();
        for inst in dmg {
            let Some(target) = self.units.get(inst.target) else {
                continue;
            };
            if target.hp <= 0 || target.invulnerable {
                continue;
            }
            let mitigated = mitigate(
                inst.amount,
                inst.kind,
                target.armor,
                target.magic_resist_pct,
            );
            let applied = mitigated.min(target.hp);
            let (pos, target_team, target_is_hero, target_is_structure) = (
                target.pos,
                target.team,
                target.hero.is_some(),
                target.is_structure(),
            );
            let t = self.units.get_mut(inst.target).expect("looked up above");
            t.hp -= applied;
            let dead = t.hp <= 0;
            if let Some(slot) = inst.slot
                && target_team != inst.team
                && let Some(seat) = self.seats.iter_mut().find(|s| s.slot == slot)
            {
                if target_is_hero {
                    seat.hero_damage += applied;
                }
                if target_is_structure {
                    seat.structure_damage += applied;
                }
            }
            events.push(Event {
                kind: EventKind::Damaged {
                    source: inst.source,
                    target: inst.target,
                    amount: applied,
                    kind: inst.kind,
                    crit: false,
                },
                visible_to: self.point_visibility(pos, target_team),
            });
            if dead {
                deaths.push(Death {
                    id: inst.target,
                    killer_unit: inst.source,
                    killer_slot: inst.slot,
                    killer_team: inst.team,
                });
            }
        }
        deaths
    }
}

/// Damage after armor or magic resistance.
fn mitigate(amount: i32, kind: DamageKind, armor: i32, magic_resist_pct: i32) -> i32 {
    match kind {
        DamageKind::Physical => {
            let den = 100 + rules::ARMOR_SCALE * armor.max(0);
            (i64::from(amount) * 100 / i64::from(den)) as i32
        }
        DamageKind::Magical => {
            let kept = (100 - magic_resist_pct).clamp(0, 100);
            (i64::from(amount) * i64::from(kept) / 100) as i32
        }
        DamageKind::Pure => amount,
    }
}
