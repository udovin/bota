//! The jungle: filling camps, and what a neutral does once drawn off one.

use bota_proto::{Team, Vec2};

use crate::engine::{CampHome, NeutralAi, Orders, UnitOrder, Upgrades, World, rosters_of};
use crate::sim::{Purpose, rules};

impl World {
    /// Fills every empty, unblocked camp on the minute mark.
    ///
    /// A camp with anything standing in its box puts nothing out, which is
    /// what makes blocking a camp work.
    pub fn fill_camps(&mut self) {
        if self.tick < rules::FIRST_NEUTRAL_TICK
            || !(self.tick - rules::FIRST_NEUTRAL_TICK)
                .is_multiple_of(rules::NEUTRAL_SPAWN_PERIOD_TICKS)
        {
            return;
        }
        let box_radius = rules::units(rules::CAMP_BOX_RADIUS);
        let upgrades = self.jungle_upgrades();
        for (index, camp) in self.map.camps.iter().enumerate() {
            if self.anything_in(camp.pos, box_radius) {
                continue;
            }
            let choices: Vec<(u8, &crate::engine::Roster)> = rosters_of(camp.kind).collect();
            let last = self.camp_last.get(index).copied().unwrap_or(u8::MAX);
            let fresh: Vec<&(u8, &crate::engine::Roster)> =
                choices.iter().filter(|(id, _)| *id != last).collect();
            if fresh.is_empty() {
                continue;
            }
            let taken = fresh[self
                .rng
                .global(Purpose::NeutralSpawn)
                .below(fresh.len() as u32) as usize];
            while self.camp_last.len() <= index {
                self.camp_last.push(u8::MAX);
            }
            self.camp_last[index] = taken.0;
            let creeps = taken.1.creeps;
            for (slot, kind) in creeps.iter().enumerate() {
                let across =
                    (slot as i32 - (creeps.len() as i32 - 1) / 2) * rules::CAMP_SPAWN_SPACING;
                let at = camp.pos + Vec2::from_ints(across, 0);
                let beast = self.spawn_unit(kind.def(), Team::Neutral, at);
                self.upgrades.insert(beast, Upgrades(upgrades));
                self.camp_home.insert(
                    beast,
                    CampHome {
                        camp: index as u8,
                        home: at,
                    },
                );
                self.neutral_ai.insert(
                    beast,
                    NeutralAi {
                        leash_left: rules::NEUTRAL_AGGRO_WINDOW,
                        reaggro_block: 0,
                        next_window: rules::NEUTRAL_AGGRO_WINDOW,
                        going_home: false,
                    },
                );
            }
        }
        self.settle();
    }

    /// How many upgrade intervals the jungle carries by now, capped.
    fn jungle_upgrades(&self) -> u32 {
        let periods = self.tick / rules::NEUTRAL_UPGRADE_PERIOD_TICKS;
        periods.min(rules::NEUTRAL_UPGRADE_CAP as u32)
    }

    /// Whether anything that walks stands inside a circle.
    fn anything_in(&self, at: Vec2, radius: bota_proto::Fixed) -> bool {
        self.entities.iter().any(|entity| {
            self.stats
                .get(entity)
                .is_some_and(|s| s.move_speed > bota_proto::Fixed::ZERO)
                && self
                    .transform
                    .get(entity)
                    .is_some_and(|t| t.pos.within(at, radius))
        })
    }

    /// Runs down every neutral's patience and sends home the ones led too far.
    ///
    /// A neutral gives up when it has been led past its guard distance for
    /// longer than its window, and takes nothing on again until it is back and
    /// its block has run out.
    pub fn tick_jungle(&mut self) {
        let guard = rules::units(rules::NEUTRAL_GUARD_DISTANCE);
        let back = rules::units(rules::NEUTRAL_RETURN);
        for entity in self.entities.iter().collect::<Vec<_>>() {
            let (Some(mut ai), Some(home), Some(at)) = (
                self.neutral_ai.get(entity).copied(),
                self.camp_home.get(entity).copied(),
                self.transform.get(entity).map(|t| t.pos),
            ) else {
                continue;
            };
            ai.reaggro_block = ai.reaggro_block.saturating_sub(1);
            if ai.going_home {
                if at.within(home.home, back) {
                    ai.going_home = false;
                    ai.leash_left = ai.next_window;
                    self.orders.insert(
                        entity,
                        Orders {
                            current: UnitOrder::Idle,
                            cooldown: 0,
                        },
                    );
                } else {
                    self.target.remove(entity);
                    self.orders.insert(
                        entity,
                        Orders {
                            current: UnitOrder::Move { pos: home.home },
                            cooldown: 0,
                        },
                    );
                }
                self.neutral_ai.insert(entity, ai);
                continue;
            }
            if at.within(home.home, guard) {
                ai.leash_left = ai.next_window;
            } else {
                ai.leash_left = ai.leash_left.saturating_sub(1);
                if ai.leash_left == 0 {
                    ai.going_home = true;
                    ai.reaggro_block = rules::NEUTRAL_REAGGRO_BLOCK;
                    ai.next_window = rules::NEUTRAL_SHORT_WINDOW;
                    self.target.remove(entity);
                }
            }
            if !ai.going_home
                && let Some(target) = self.target_of(entity)
                && self.alive(target)
            {
                let Some(their_at) = self.transform.get(target).map(|t| t.pos) else {
                    continue;
                };
                self.orders.insert(
                    entity,
                    Orders {
                        current: UnitOrder::AttackMove { pos: their_at },
                        cooldown: 0,
                    },
                );
            }
            self.neutral_ai.insert(entity, ai);
        }
    }
}
