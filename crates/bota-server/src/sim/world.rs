//! The authoritative game state.

use bota_proto::{EntityId, HeroId, MatchStats, SlotId, SlotStats, Team, UnitKind, Vec2};

use crate::sim::{
    Arena, Chance, Ground, MatchConfig, MatchRng, PassGrid, Projectile, SeatState, Stream, Unit,
    UnitOrder, Windup, rules,
};

/// Everything the simulation knows.
///
/// Only [`WorldView`](bota_proto::WorldView) projections of this ever leave the
/// server.
#[derive(Clone, Debug)]
pub struct World {
    /// Completed ticks since the match started.
    pub tick: u32,
    /// Every unit alive.
    pub units: Arena<Unit>,
    /// Every projectile in flight.
    pub projectiles: Arena<Projectile>,
    /// One entry per seat, sorted by slot.
    pub seats: Vec<SeatState>,
    /// The walkability grid.
    pub grid: PassGrid,
    /// The terrain: elevation tiers, water, ground walkability.
    pub ground: Ground,
    /// Cells that block sight lines: trees and the fog blocker walls.
    pub tree_cover: PassGrid,
    /// The shared uphill miss chance of ranged attacks.
    pub uphill: Chance,
    /// Ticks until Roshan returns to his pit. Zero while he lives.
    pub roshan_respawn: u32,
    /// The hidden draws behind Roshan's respawn waits.
    pub roshan_rng: Stream,
    /// The hidden randomness of the match.
    pub rng: MatchRng,
    /// Which side has won. The world stops stepping once set.
    pub winner: Option<Team>,
    /// The map being played.
    pub map: &'static crate::sim::MapDef,
    /// The roster each camp filled with last, so it never repeats one.
    pub camp_last: Vec<u8>,
}

impl World {
    /// A world at tick zero: buildings standing, heroes at their fountains.
    ///
    /// `rng` is initial hidden state, not configuration; deriving it is the
    /// server's business.
    pub fn new(cfg: &MatchConfig, rng: MatchRng) -> World {
        let map = crate::sim::map_of(cfg.map);
        let uphill = Chance::new(rng.global(crate::sim::Purpose::Evasion), rules::UPHILL_MISS);
        let roshan_rng = rng.global(crate::sim::Purpose::Roshan);
        let mut world = World {
            tick: 0,
            units: Arena::new(),
            projectiles: Arena::new(),
            seats: Vec::new(),
            grid: PassGrid::open(),
            ground: Ground::of(map),
            tree_cover: crate::sim::build_sight_block(map),
            uphill,
            roshan_respawn: 0,
            roshan_rng,
            rng,
            winner: None,
            map,
            camp_last: vec![u8::MAX; map.camps.len()],
        };
        // The terrain closes its own ground: cliffs, pits, the map edge.
        for cy in 0..rules::GRID_CELLS {
            for cx in 0..rules::GRID_CELLS {
                if !world.ground.cell_walkable(cx, cy) {
                    world.grid.close_cell(cx, cy);
                }
            }
        }
        for team in [Team::Radiant, Team::Dire] {
            world
                .units
                .insert(Unit::fountain(team, map.fountains[team_index(team)]));
            world
                .units
                .insert(Unit::ancient(team, map.ancients[team_index(team)]));
        }
        for &(lane, tier, pos) in map.radiant_towers {
            world
                .units
                .insert(Unit::tower(Team::Radiant, pos, lane, tier));
        }
        for &(lane, tier, pos) in map.dire_towers {
            world.units.insert(Unit::tower(Team::Dire, pos, lane, tier));
        }
        let footprints: Vec<(Vec2, bota_proto::Fixed)> = world
            .units
            .iter()
            .filter(|(_, u)| u.is_structure())
            .map(|(_, u)| (u.pos, u.radius))
            .collect();
        for (pos, radius) in footprints {
            world
                .grid
                .block_circle(pos, crate::sim::structure_clearance(radius));
        }
        for pos in tree_positions(map) {
            world.grid.block_circle(
                pos,
                crate::sim::structure_clearance(rules::units(rules::TREE_RADIUS)),
            );
        }
        for pick in &cfg.picks {
            world.seats.push(SeatState {
                slot: pick.slot,
                team: pick.team,
                hero: pick.hero,
                unit: None,
                gold: rules::STARTING_GOLD,
                net_worth: rules::STARTING_GOLD,
                xp: 0,
                level: 1,
                kills: 0,
                deaths: 0,
                assists: 0,
                last_hits: 0,
                denies: 0,
                hero_damage: 0,
                structure_damage: 0,
                respawn_left: 0,
                kill_streak: 0,
                items: vec![None; crate::sim::TOTAL_SLOTS],
                abilities: crate::sim::hero_kit(),
            });
        }
        for i in 0..world.seats.len() {
            world.spawn_hero(i);
        }
        world.units.insert(Unit::roshan());
        world
    }

    /// Puts a seat's hero on the map at its fountain.
    pub fn spawn_hero(&mut self, seat_index: usize) {
        let seat = &self.seats[seat_index];
        let unit = Unit::hero(
            seat.team,
            seat.slot,
            seat.hero,
            seat.level,
            hero_spawn_pos(self.map, seat.team),
        );
        let id = self.units.insert(unit);
        // The crit stream is keyed by the arena slot, so a respawned hero
        // continues its sequence rather than restarting it.
        let chance = crate::sim::Chance::new(
            self.rng.for_unit(crate::sim::Purpose::Crit, id, 0),
            crate::sim::Ratio::NEVER,
        );
        self.units.get_mut(id).expect("just inserted").crit = Some(chance);
        self.seats[seat_index].unit = Some(id);
        self.seats[seat_index].respawn_left = 0;
    }

    /// The seat behind a slot, if the slot is in the match.
    pub fn seat(&self, slot: SlotId) -> Option<&SeatState> {
        self.seats.iter().find(|s| s.slot == slot)
    }

    /// The mutable seat behind a slot, if the slot is in the match.
    pub fn seat_mut(&mut self, slot: SlotId) -> Option<&mut SeatState> {
        self.seats.iter_mut().find(|s| s.slot == slot)
    }

    /// Which side has won, if any.
    pub fn winner(&self) -> Option<Team> {
        self.winner
    }

    /// Final numbers for every seat.
    pub fn stats(&self) -> MatchStats {
        MatchStats {
            duration: self.tick,
            slots: self
                .seats
                .iter()
                .map(|s| SlotStats {
                    slot: s.slot,
                    kills: s.kills,
                    deaths: s.deaths,
                    assists: s.assists,
                    last_hits: s.last_hits,
                    denies: s.denies,
                    net_worth: s.net_worth,
                    hero_damage: s.hero_damage,
                    structure_damage: s.structure_damage,
                })
                .collect(),
        }
    }

    /// FNV-1a over the whole state, hidden parts included.
    ///
    /// Two worlds that stepped the same commands from the same seed hash the
    /// same on every platform. Covers every field that influences a future
    /// tick, so a divergence moves the hash on the tick it happens.
    pub fn hash(&self) -> u64 {
        let mut f = Fnv::new();
        f.u32(self.tick);
        f.u32(self.roshan_respawn);
        match self.winner {
            None => f.u8(0),
            Some(team) => {
                f.u8(1);
                f.u8(team_code(team));
            }
        }
        for (id, unit) in self.units.iter() {
            hash_entity(&mut f, id);
            hash_unit(&mut f, unit);
        }
        for (id, p) in self.projectiles.iter() {
            hash_entity(&mut f, id);
            f.u8(team_code(p.team));
            hash_vec2(&mut f, p.pos);
            f.i32(p.speed.raw);
            f.i32(p.damage);
            hash_entity(&mut f, p.target);
            match p.source {
                None => f.u8(0),
                Some(src) => {
                    f.u8(1);
                    hash_entity(&mut f, src);
                }
            }
        }
        for seat in &self.seats {
            f.u8(seat.slot.0);
            f.u8(team_code(seat.team));
            f.u32(u32::from(seat.hero.0));
            match seat.unit {
                None => f.u8(0),
                Some(id) => {
                    f.u8(1);
                    hash_entity(&mut f, id);
                }
            }
            f.i32(seat.gold);
            f.i32(seat.net_worth);
            f.i32(seat.xp);
            f.u8(seat.level);
            f.u32(u32::from(seat.kills));
            f.u32(u32::from(seat.deaths));
            f.u32(u32::from(seat.last_hits));
            f.u32(u32::from(seat.denies));
            f.i32(seat.hero_damage);
            f.i32(seat.structure_damage);
            f.u32(seat.respawn_left);
            f.i32(seat.kill_streak);
            for stack in &seat.items {
                match stack {
                    None => f.u8(0),
                    Some(s) => {
                        f.u8(1);
                        f.u32(u32::from(s.id.0));
                        f.u32(u32::from(s.charges));
                        f.u32(s.cooldown);
                        f.u32(s.bought_tick);
                        f.u8(u8::from(s.touched));
                    }
                }
            }
            for a in &seat.abilities {
                f.u32(u32::from(a.id.0));
                f.u32(u32::from(a.level));
                f.u32(a.cooldown);
            }
        }
        f.finish()
    }
}

/// The fountain position of a team. The jungle's is the map center: it has
/// no fountain, and nothing ever stands there.
pub fn fountain_pos(map: &crate::sim::MapDef, team: Team) -> Vec2 {
    match team {
        Team::Radiant => map.fountains[0],
        Team::Dire => map.fountains[1],
        Team::Neutral => Vec2::from_ints(rules::MAP_SIZE / 2, rules::MAP_SIZE / 2),
    }
}

/// Where a team's hero appears, beside the fountain rather than inside it.
pub fn hero_spawn_pos(map: &crate::sim::MapDef, team: Team) -> Vec2 {
    let offset = match team {
        Team::Radiant => Vec2::from_ints(rules::HERO_SPAWN_OFFSET, rules::HERO_SPAWN_OFFSET),
        Team::Dire => Vec2::from_ints(-rules::HERO_SPAWN_OFFSET, -rules::HERO_SPAWN_OFFSET),
        Team::Neutral => Vec2::ZERO,
    };
    fountain_pos(map, team) + offset
}

/// The Ancient position of a team.
pub fn ancient_pos(map: &crate::sim::MapDef, team: Team) -> Vec2 {
    match team {
        Team::Radiant => map.ancients[0],
        Team::Dire => map.ancients[1],
        Team::Neutral => Vec2::from_ints(rules::MAP_SIZE / 2, rules::MAP_SIZE / 2),
    }
}

/// The mirror of a position through the map center.
pub fn mirror(pos: Vec2) -> Vec2 {
    Vec2::from_ints(rules::MAP_SIZE, rules::MAP_SIZE) - pos
}

/// Every tree on the map: the real Dota forest, with this map's own lane
/// corridors and bases kept clear so the straightened lanes stay walkable.
pub fn tree_positions(map: &crate::sim::MapDef) -> Vec<Vec2> {
    if !map.trees {
        return Vec::new();
    }
    let lane_clear = {
        let r = i64::from(rules::units(rules::TREE_LANE_CLEAR).raw);
        r * r
    };
    let base_clear = rules::units(rules::TREE_BASE_CLEAR);
    crate::sim::DOTA_TREES
        .iter()
        .map(|&(x, y)| Vec2::from_ints(i32::from(x), i32::from(y)))
        .filter(|&pos| {
            for lane in map.lanes() {
                if lane_offset_squared(map, lane, pos) < lane_clear {
                    return false;
                }
            }
            !pos.within(map.fountains[0], base_clear)
                && !pos.within(rules::DIRE_FOUNTAIN_POS, base_clear)
        })
        .collect()
}

/// The lane the diagonal mirror turns a lane into: the sides swap.
pub fn mirrored_lane(lane: u8) -> u8 {
    match lane {
        rules::LANE_TOP => rules::LANE_BOT,
        rules::LANE_BOT => rules::LANE_TOP,
        other => other,
    }
}

/// The physical centerline of a lane, Radiant base first.
///
/// The line runs through every tower of the lane, so a wave walks from tower
/// to tower and cannot wander past one out of its own acquisition range.
pub fn lane_polyline(map: &crate::sim::MapDef, lane: u8) -> Vec<Vec2> {
    let tower_of = |table: &[(u8, u8, Vec2)], tier: u8| {
        table
            .iter()
            .find(|&&(tl, tt, _)| tl == lane && tt == tier)
            .map(|&(_, _, pos)| pos)
    };
    let mut line = vec![map.ancients[0]];
    for tier in [3u8, 2, 1] {
        if let Some(pos) = tower_of(map.radiant_towers, tier) {
            line.push(pos);
        }
    }
    if let Some(corners) = map.lane_corners.get(usize::from(lane)) {
        line.extend_from_slice(corners);
    }
    for tier in [1u8, 2, 3] {
        if let Some(pos) = tower_of(map.dire_towers, tier) {
            line.push(pos);
        }
    }
    line.push(map.ancients[1]);
    line
}

/// The waypoints a team's creeps push through on a lane, enemy Ancient last.
pub fn lane_route(map: &crate::sim::MapDef, team: Team, lane: u8) -> Vec<Vec2> {
    let mut line = lane_polyline(map, lane);
    if team == Team::Dire {
        line.reverse();
    }
    line.remove(0);
    line
}

/// The passability grid of a map: its terrain, its buildings and its forest.
///
/// Built from the map alone, so the routes found on it never depend on which
/// world asked first.
pub fn build_grid(map: &crate::sim::MapDef) -> PassGrid {
    let ground = Ground::of(map);
    let mut grid = PassGrid::open();
    for cy in 0..rules::GRID_CELLS {
        for cx in 0..rules::GRID_CELLS {
            if !ground.cell_walkable(cx, cy) {
                grid.close_cell(cx, cy);
            }
        }
    }
    let mut block = |pos: Vec2, radius: bota_proto::Fixed| {
        grid.block_circle(pos, crate::sim::structure_clearance(radius));
    };
    for at in map.fountains {
        block(at, rules::units(rules::FOUNTAIN_RADIUS));
    }
    for at in map.ancients {
        block(at, rules::units(rules::ANCIENT_RADIUS));
    }
    for &(_, _, at) in map.radiant_towers.iter().chain(map.dire_towers) {
        block(at, rules::units(rules::TOWER_RADIUS));
    }
    for at in tree_positions(map) {
        block(at, rules::units(rules::TREE_RADIUS));
    }
    grid
}

/// The landmarks a team's creeps march through on a lane, spawner first.
fn lane_landmarks(map: &crate::sim::MapDef, team: Team, lane: u8) -> Vec<Vec2> {
    let mut line = vec![creep_spawn_pos(map, team, lane)];
    line.extend(lane_route(map, team, lane));
    line
}

/// The walked route of every lane, both sides, indexed by team then lane.
///
/// Every match runs the same map, so the routes are found once and shared.
pub fn lane_routes(map: &'static crate::sim::MapDef) -> &'static [[Vec<Vec2>; 3]; 2] {
    static ROUTES: std::sync::OnceLock<Vec<[[Vec<Vec2>; 3]; 2]>> = std::sync::OnceLock::new();
    &ROUTES.get_or_init(|| {
        crate::sim::MAPS
            .iter()
            .map(|m| {
                let grid = build_grid(m);
                let build = |team: Team| {
                    [
                        walk_lane(m, &grid, team, rules::LANE_MID),
                        walk_lane(m, &grid, team, rules::LANE_TOP),
                        walk_lane(m, &grid, team, rules::LANE_BOT),
                    ]
                };
                [build(Team::Radiant), build(Team::Dire)]
            })
            .collect()
    })[map.index()]
}

/// One lane's walked route: the landmarks, with a found path laid between
/// each pair so the march goes around what stands in the way.
fn walk_lane(map: &crate::sim::MapDef, grid: &PassGrid, team: Team, lane: u8) -> Vec<Vec2> {
    let marks = lane_landmarks(map, team, lane);
    let mut out = Vec::new();
    for leg in marks.windows(2) {
        out.extend(crate::sim::find_path(grid, leg[0], leg[1]));
        // Landmarks are tower positions, and a tower closes the ground it
        // stands on: the march aims beside it, not at it.
        out.push(crate::sim::nearest_open(grid, leg[1]));
    }
    out
}

/// Squared distance from a lane's centerline.
pub fn lane_offset_squared(map: &crate::sim::MapDef, lane: u8, pos: Vec2) -> i64 {
    let line = lane_polyline(map, lane);
    line.windows(2)
        .map(|s| crate::sim::segment_distance_squared(pos, s[0], s[1]))
        .min()
        .expect("a lane has at least one segment")
}

/// The nearest point of a lane's centerline.
pub fn lane_return_point(map: &crate::sim::MapDef, lane: u8, pos: Vec2) -> Vec2 {
    let line = lane_polyline(map, lane);
    line.windows(2)
        .map(|s| crate::sim::segment_nearest(pos, s[0], s[1]))
        .min_by_key(|p| pos.distance_squared(*p))
        .expect("a lane has at least one segment")
}

/// The creep spawn position of a team on a lane. The jungle runs no lanes.
pub fn creep_spawn_pos(map: &crate::sim::MapDef, team: Team, lane: u8) -> Vec2 {
    match team {
        Team::Radiant => map.creep_spawns[0][usize::from(lane)],
        Team::Dire => map.creep_spawns[1][usize::from(lane)],
        Team::Neutral => Vec2::from_ints(rules::MAP_SIZE / 2, rules::MAP_SIZE / 2),
    }
}

/// Where a team sits in the per-team route tables. The jungle marches
/// nowhere and answers zero.
pub fn team_index(team: Team) -> usize {
    match team {
        Team::Radiant | Team::Neutral => 0,
        Team::Dire => 1,
    }
}

/// The side a team's creeps push towards. The jungle pushes nowhere.
pub fn enemy_of(team: Team) -> Team {
    match team {
        Team::Radiant => Team::Dire,
        Team::Dire => Team::Radiant,
        Team::Neutral => Team::Neutral,
    }
}

/// FNV-1a, folded 64-bit.
///
/// Hand-rolled so the value is identical on every platform and every release;
/// the standard hasher promises neither.
pub struct Fnv {
    state: u64,
}

impl Fnv {
    /// The standard FNV-1a offset basis.
    pub fn new() -> Fnv {
        Fnv {
            state: 0xcbf2_9ce4_8422_2325,
        }
    }

    /// Folds one byte in.
    pub fn u8(&mut self, byte: u8) {
        self.state ^= u64::from(byte);
        self.state = self.state.wrapping_mul(0x0000_0100_0000_01b3);
    }

    /// Folds four bytes in, little endian.
    pub fn u32(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.u8(b);
        }
    }

    /// Folds four bytes in, little endian.
    pub fn i32(&mut self, v: i32) {
        self.u32(v as u32)
    }

    /// The accumulated hash.
    pub fn finish(&self) -> u64 {
        self.state
    }
}

impl Default for Fnv {
    fn default() -> Fnv {
        Fnv::new()
    }
}

fn team_code(team: Team) -> u8 {
    match team {
        Team::Radiant => 0,
        Team::Dire => 1,
        Team::Neutral => 2,
    }
}

fn hash_entity(f: &mut Fnv, id: EntityId) {
    f.u32(id.idx);
    f.u32(id.generation);
}

fn hash_vec2(f: &mut Fnv, v: Vec2) {
    f.i32(v.x.raw);
    f.i32(v.y.raw);
}

fn hash_unit(f: &mut Fnv, unit: &Unit) {
    f.u8(kind_code(unit.kind));
    f.u8(team_code(unit.team));
    hash_vec2(f, unit.pos);
    f.u32(u32::from(unit.facing.brads));
    f.i32(unit.hp);
    f.i32(unit.max_hp);
    f.i32(unit.mana);
    f.i32(unit.max_mana);
    f.i32(unit.move_speed.raw);
    f.i32(unit.attack_damage);
    f.i32(unit.attack_range.raw);
    f.i32(unit.acquisition_range.raw);
    f.u32(unit.attack_interval);
    f.u32(unit.attack_point);
    match unit.projectile_speed {
        None => f.u8(0),
        Some(s) => {
            f.u8(1);
            f.i32(s.raw);
        }
    }
    f.i32(unit.armor);
    f.i32(unit.magic_resist_pct);
    f.i32(unit.radius.raw);
    f.i32(unit.vision_radius.raw);
    f.u8(u8::from(unit.invulnerable));
    hash_order(f, unit.order);
    match unit.engage {
        None => f.u8(0),
        Some(id) => {
            f.u8(1);
            hash_entity(f, id);
        }
    }
    match unit.windup {
        None => f.u8(0),
        Some(Windup { target, ticks_left }) => {
            f.u8(1);
            hash_entity(f, target);
            f.u32(ticks_left);
        }
    }
    f.u32(unit.attack_cooldown);
    f.u32(unit.attack_backswing);
    f.u32(unit.recovering);
    f.u32(unit.frenzy_ticks);
    f.i32(unit.frenzy_pct);
    f.u32(unit.salve_ticks);
    f.u32(unit.clarity_ticks);
    f.i32(unit.item_bonus.move_speed);
    f.i32(unit.item_bonus.damage);
    f.i32(unit.item_bonus.armor);
    f.i32(unit.item_bonus.hp);
    f.i32(unit.item_bonus.mana);
    hash_vec2(f, unit.path_goal);
    f.u32(unit.path.len() as u32);
    for w in &unit.path {
        hash_vec2(f, *w);
    }
    f.u32(unit.order_cooldown);
    f.u8(u8::from(unit.returning));
    hash_vec2(f, unit.camp);
    match unit.owner {
        None => f.u8(0),
        Some(SlotId(s)) => {
            f.u8(1);
            f.u8(s);
        }
    }
    match unit.hero {
        None => f.u8(0),
        Some(HeroId(h)) => {
            f.u8(1);
            f.u32(u32::from(h));
        }
    }
    f.u8(unit.level);
    f.u8(unit.lane);
    match &unit.ai {
        None => f.u8(0),
        Some(crate::sim::CreepAi::Neutral(ai)) => {
            f.u8(2);
            f.u8(ai.camp);
            hash_vec2(f, ai.home);
            f.u32(ai.leash_left);
            f.u32(ai.reaggro_block);
            f.u32(ai.next_window);
            f.u8(u8::from(ai.going_home));
        }
        Some(crate::sim::CreepAi::Lane(ai)) => {
            f.u8(1);
            f.u32(u32::from(ai.step));
            f.u32(ai.chase_left);
            f.u32(ai.provoked);
            match ai.anchor {
                None => f.u8(0),
                Some(p) => {
                    f.u8(1);
                    hash_vec2(f, p);
                }
            }
            match ai.last_seen {
                None => f.u8(0),
                Some(p) => {
                    f.u8(1);
                    hash_vec2(f, p);
                }
            }
        }
    }
    f.u8(unit.tier);
    f.i32(unit.bounty);
    f.i32(unit.xp_reward);
}

fn hash_order(f: &mut Fnv, order: UnitOrder) {
    match order {
        UnitOrder::Idle => f.u8(0),
        UnitOrder::Hold => f.u8(1),
        UnitOrder::Move { pos } => {
            f.u8(2);
            hash_vec2(f, pos);
        }
        UnitOrder::AttackMove { pos } => {
            f.u8(3);
            hash_vec2(f, pos);
        }
        UnitOrder::Attack { target, last_seen } => {
            f.u8(4);
            hash_entity(f, target);
            hash_vec2(f, last_seen);
        }
    }
}

fn kind_code(kind: UnitKind) -> u8 {
    match kind {
        UnitKind::Hero => 0,
        UnitKind::CreepMelee => 1,
        UnitKind::CreepRanged => 2,
        UnitKind::CreepSiege => 3,
        UnitKind::CreepNeutral => 4,
        UnitKind::Tower => 5,
        UnitKind::Ancient => 6,
        UnitKind::Fountain => 7,
        UnitKind::Ward => 8,
        UnitKind::Roshan => 9,
        UnitKind::CreepFlagbearer => 10,
    }
}
