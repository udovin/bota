//! Waves: putting them on the map and walking them down their lane.

use bota_proto::{Team, Vec2};

use crate::engine::{Lane, LaneAi, March, Orders, UnitDef, UnitOrder, Upgrades, World};
use crate::sim::{
    Purpose, WavePlan, advance_waypoint, creep_spawn_pos, lane_routes, rules, spawn_offsets,
    team_index, wave_at, wave_plan,
};

impl World {
    /// Puts a wave in every lane when the clock calls for one.
    pub fn spawn_waves(&mut self) {
        let Some(wave) = wave_at(self.tick) else {
            return;
        };
        let plan = wave_plan(wave);
        let map = self.map;
        for team in [Team::Radiant, Team::Dire] {
            for lane in map.lanes() {
                let at = creep_spawn_pos(map, team, lane);
                let route = &lane_routes(map)[team_index(team)][usize::from(lane)];
                let forward = route
                    .iter()
                    .find(|w| !w.within(at, rules::units(rules::WAVE_FACING_LOOKAHEAD)))
                    .map_or(Vec2::ZERO, |w| *w - at);
                let offsets = spawn_offsets(&plan, forward);
                let flag_slot = self.flag_slot(plan.melee);
                for (index, def) in wave_ranks(&plan, flag_slot).into_iter().enumerate() {
                    let pos = at + offsets.get(index).copied().unwrap_or(Vec2::ZERO);
                    let creep = self.spawn_unit(def, team, pos);
                    self.lane.insert(creep, Lane(lane));
                    self.upgrades.insert(creep, Upgrades(plan.upgrades));
                    self.march.insert(
                        creep,
                        March {
                            route_step: 0,
                            trace: None,
                            shove: 0,
                        },
                    );
                    self.lane_ai.insert(
                        creep,
                        LaneAi {
                            anchor: None,
                            chase_left: 0,
                            provoked: 0,
                            last_seen: None,
                        },
                    );
                }
            }
        }
        self.settle();
    }

    /// Which melee creep of a wave carries the flag.
    fn flag_slot(&mut self, melee: u32) -> u32 {
        if melee == 0 {
            return 0;
        }
        self.rng.global(Purpose::Wave).below(melee)
    }

    /// Sends every creep where it should be walking.
    ///
    /// This is the one place a creep's order is written. What it is set on
    /// comes first, then the spot its target was last seen, then the place it
    /// left its route, and only then the route itself.
    pub fn march_lanes(&mut self) {
        let map = self.map;
        for entity in self.entities.iter().collect::<Vec<_>>() {
            let Some(mut march) = self.march.get(entity).copied() else {
                continue;
            };
            let Some(at) = self.transform.get(entity).map(|t| t.pos) else {
                continue;
            };
            let chasing = self
                .engage
                .get(entity)
                .copied()
                .filter(|target| self.alive(*target))
                .and_then(|target| self.transform.get(target).map(|t| t.pos));
            let ai = self.lane_ai.get(entity).copied();
            let going = chasing
                .or_else(|| ai.and_then(|ai| ai.last_seen))
                .or_else(|| ai.and_then(|ai| ai.anchor))
                .or_else(|| {
                    let (team, lane) = (
                        self.team.get(entity).copied()?,
                        self.lane.get(entity).copied()?,
                    );
                    let route = &lane_routes(map)[team_index(team)][usize::from(lane.0)];
                    if route.is_empty() {
                        return None;
                    }
                    let step = advance_waypoint(route, usize::from(march.route_step), at);
                    march.route_step = step as u16;
                    self.march.insert(entity, march);
                    Some(route[step])
                });
            let Some(going) = going else {
                continue;
            };
            self.orders.insert(
                entity,
                Orders {
                    current: UnitOrder::AttackMove { pos: going },
                    cooldown: 0,
                },
            );
        }
    }
}

/// The kinds a wave is made of, in the order they are placed.
fn wave_ranks(plan: &WavePlan, flag_slot: u32) -> Vec<&'static UnitDef> {
    let mut out = Vec::new();
    let front = plan.melee + plan.siege;
    let siege_at = front / 2;
    let mut melee_seen = 0;
    for index in 0..front {
        if index >= siege_at && index < siege_at + plan.siege {
            out.push(&crate::engine::SIEGE_CREEP);
            continue;
        }
        let flagged = plan.flagbearer && melee_seen == flag_slot;
        melee_seen += 1;
        out.push(if flagged {
            &crate::engine::FLAGBEARER_CREEP
        } else {
            &crate::engine::MELEE_CREEP
        });
    }
    for _ in 0..plan.ranged {
        out.push(&crate::engine::RANGED_CREEP);
    }
    out
}
