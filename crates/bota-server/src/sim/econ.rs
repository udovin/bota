//! Gold, experience, levels, deaths and respawns.

use bota_proto::{EventKind, Team, UnitKind, Vec2};

use crate::sim::{Death, Event, EventVisibility, Unit, World, enemy_of, fountain_pos, rules};

impl World {
    /// Hands out the periodic passive gold.
    pub fn passive_gold(&mut self) {
        if self.tick.is_multiple_of(rules::PASSIVE_GOLD_PERIOD_TICKS) {
            for seat in &mut self.seats {
                seat.gold += 1;
                seat.net_worth += 1;
            }
        }
    }

    /// Counts down respawn timers and puts heroes back on the map.
    pub fn tick_respawns(&mut self) {
        for i in 0..self.seats.len() {
            if self.seats[i].respawn_left > 0 {
                self.seats[i].respawn_left -= 1;
                if self.seats[i].respawn_left == 0 {
                    self.spawn_hero(i);
                }
            }
        }
    }

    /// Passive regeneration and the fountain area.
    pub fn regen(&mut self) {
        let hp_tick = self.tick.is_multiple_of(rules::HERO_HP_REGEN_PERIOD);
        let mana_tick = self.tick.is_multiple_of(rules::HERO_MANA_REGEN_PERIOD);
        let heal_radius = rules::units(rules::FOUNTAIN_HEAL_RADIUS);
        for (_, unit) in self.units.iter_mut() {
            if unit.kind != UnitKind::Hero {
                continue;
            }
            if hp_tick {
                unit.hp = (unit.hp + 1).min(unit.max_hp);
            }
            if mana_tick {
                unit.mana = (unit.mana + 1).min(unit.max_mana);
            }
            if unit.pos.within(fountain_pos(unit.team), heal_radius) {
                unit.hp = (unit.hp + rules::FOUNTAIN_HEAL_HP_PER_TICK).min(unit.max_hp);
                unit.mana = (unit.mana + rules::FOUNTAIN_HEAL_MANA_PER_TICK).min(unit.max_mana);
            }
        }
    }

    /// Spawns the creep wave due this tick, if one is.
    pub fn spawn_waves(&mut self) {
        if self.tick < rules::FIRST_WAVE_TICK
            || !(self.tick - rules::FIRST_WAVE_TICK).is_multiple_of(rules::WAVE_PERIOD_TICKS)
        {
            return;
        }
        let wave = (self.tick - rules::FIRST_WAVE_TICK) / rules::WAVE_PERIOD_TICKS + 1;
        let with_siege = wave.is_multiple_of(rules::SIEGE_WAVE_PERIOD);
        for team in [Team::Radiant, Team::Dire] {
            let spawn = crate::sim::creep_spawn_pos(team);
            for i in 0..rules::MELEE_PER_WAVE {
                let pos = spawn + rules::WAVE_SPAWN_OFFSETS[i as usize];
                self.units.insert(Unit::melee_creep(team, pos));
            }
            let ranged_pos = spawn + rules::WAVE_SPAWN_OFFSETS[rules::MELEE_PER_WAVE as usize];
            self.units.insert(Unit::ranged_creep(team, ranged_pos));
            if with_siege {
                let siege_pos =
                    spawn + rules::WAVE_SPAWN_OFFSETS[rules::MELEE_PER_WAVE as usize + 1];
                self.units.insert(Unit::siege_creep(team, siege_pos));
            }
        }
    }

    /// Buries the dead: rewards, timers, victory.
    pub fn process_deaths(&mut self, deaths: Vec<Death>, events: &mut Vec<Event>) {
        for death in deaths {
            let Some(unit) = self.units.remove(death.id) else {
                continue;
            };
            let denied = death.killer_team == unit.team;
            let death_event_visibility = if unit.kind == UnitKind::Hero || unit.is_structure() {
                EventVisibility::Everyone
            } else {
                self.point_visibility(unit.pos, unit.team)
            };
            events.push(Event {
                kind: EventKind::Died {
                    unit: death.id,
                    killer: death.killer_unit,
                    denied,
                },
                visible_to: death_event_visibility,
            });
            match unit.kind {
                UnitKind::Hero => self.process_hero_death(&unit, &death, events),
                UnitKind::Tower => {
                    self.pay_gold(death.killer_slot, unit.bounty);
                    events.push(Event {
                        kind: EventKind::StructureDestroyed {
                            unit: death.id,
                            team: unit.team,
                        },
                        visible_to: EventVisibility::Everyone,
                    });
                }
                UnitKind::Ancient => {
                    events.push(Event {
                        kind: EventKind::StructureDestroyed {
                            unit: death.id,
                            team: unit.team,
                        },
                        visible_to: EventVisibility::Everyone,
                    });
                    self.winner = Some(enemy_of(unit.team));
                }
                _ => self.process_creep_death(&unit, &death, denied, events),
            }
        }
    }

    fn process_creep_death(
        &mut self,
        unit: &Unit,
        death: &Death,
        denied: bool,
        events: &mut Vec<Event>,
    ) {
        if let Some(slot) = death.killer_slot
            && let Some(seat) = self.seats.iter_mut().find(|s| s.slot == slot)
        {
            if denied {
                seat.denies += 1;
            } else {
                seat.last_hits += 1;
                seat.gold += unit.bounty;
                seat.net_worth += unit.bounty;
            }
        }
        let xp = if denied {
            unit.xp_reward * rules::DENIED_XP_PCT / 100
        } else {
            unit.xp_reward
        };
        self.grant_xp_around(unit.pos, enemy_of(unit.team), xp, events);
    }

    fn process_hero_death(&mut self, unit: &Unit, death: &Death, events: &mut Vec<Event>) {
        let victim_index = self
            .seats
            .iter()
            .position(|s| Some(s.slot) == unit.owner)
            .expect("a hero unit always has a seat");
        let victim_streak = self.seats[victim_index].kill_streak;
        let victim_level = i32::from(unit.level);
        {
            let victim = &mut self.seats[victim_index];
            victim.deaths += 1;
            victim.kill_streak = 0;
            victim.unit = None;
            victim.respawn_left =
                rules::RESPAWN_BASE_TICKS + rules::RESPAWN_PER_LEVEL_TICKS * u32::from(unit.level);
        }
        if let Some(killer_slot) = death.killer_slot
            && death.killer_team != unit.team
        {
            let bounty = rules::HERO_KILL_BOUNTY_BASE
                + rules::HERO_KILL_BOUNTY_PER_STREAK
                    * victim_streak.min(rules::HERO_KILL_STREAK_CAP);
            if let Some(killer) = self.seats.iter_mut().find(|s| s.slot == killer_slot) {
                killer.kills += 1;
                killer.kill_streak += 1;
                killer.gold += bounty;
                killer.net_worth += bounty;
            }
        }
        let xp = rules::HERO_KILL_XP_BASE + rules::HERO_KILL_XP_PER_LEVEL * victim_level;
        self.grant_xp_around(unit.pos, enemy_of(unit.team), xp, events);
    }

    fn pay_gold(&mut self, slot: Option<bota_proto::SlotId>, amount: i32) {
        if let Some(slot) = slot
            && let Some(seat) = self.seats.iter_mut().find(|s| s.slot == slot)
        {
            seat.gold += amount;
            seat.net_worth += amount;
        }
    }

    /// Grants experience to every living hero of `team` near a point.
    fn grant_xp_around(&mut self, pos: Vec2, team: Team, amount: i32, events: &mut Vec<Event>) {
        if amount <= 0 {
            return;
        }
        let radius = rules::units(rules::XP_RADIUS);
        let receivers: Vec<usize> = (0..self.seats.len())
            .filter(|&i| {
                let seat = &self.seats[i];
                seat.team == team
                    && seat
                        .unit
                        .and_then(|id| self.units.get(id))
                        .is_some_and(|u| u.pos.within(pos, radius))
            })
            .collect();
        for i in receivers {
            self.grant_xp(i, amount, events);
        }
    }

    /// Adds experience to a seat, levelling the hero up as thresholds pass.
    pub fn grant_xp(&mut self, seat_index: usize, amount: i32, events: &mut Vec<Event>) {
        let seat = &mut self.seats[seat_index];
        seat.xp += amount;
        while seat.level < rules::HERO_MAX_LEVEL
            && seat.xp >= rules::XP_THRESHOLDS[seat.level as usize]
        {
            seat.level += 1;
            let new_level = seat.level;
            let unit_id = seat.unit;
            if let Some(id) = unit_id
                && let Some(unit) = self.units.get_mut(id)
            {
                unit.level = new_level;
                unit.max_hp += rules::HERO_HP_PER_LEVEL;
                unit.hp += rules::HERO_HP_PER_LEVEL;
                unit.max_mana += rules::HERO_MANA_PER_LEVEL;
                unit.mana += rules::HERO_MANA_PER_LEVEL;
                unit.attack_damage += rules::HERO_ATTACK_DAMAGE_PER_LEVEL;
                events.push(Event {
                    kind: EventKind::LevelUp {
                        unit: id,
                        level: new_level,
                    },
                    visible_to: EventVisibility::Everyone,
                });
            }
        }
    }
}
