//! The authoritative game state.

use bota_proto::{EntityId, HeroId, MatchStats, SlotId, SlotStats, Team, UnitKind, Vec2};

use crate::sim::{
    Arena, MatchConfig, MatchRng, PassGrid, Projectile, SeatState, Unit, UnitOrder, Windup, rules,
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
    /// The hidden randomness of the match.
    pub rng: MatchRng,
    /// Which side has won. The world stops stepping once set.
    pub winner: Option<Team>,
}

impl World {
    /// A world at tick zero: buildings standing, heroes at their fountains.
    ///
    /// `rng` is initial hidden state, not configuration; deriving it is the
    /// server's business.
    pub fn new(cfg: &MatchConfig, rng: MatchRng) -> World {
        let mut world = World {
            tick: 0,
            units: Arena::new(),
            projectiles: Arena::new(),
            seats: Vec::new(),
            grid: PassGrid::open(),
            rng,
            winner: None,
        };
        for team in [Team::Radiant, Team::Dire] {
            world.units.insert(Unit::fountain(team, fountain_pos(team)));
            world.units.insert(Unit::ancient(team, ancient_pos(team)));
            world.units.insert(Unit::tower(team, tower_pos(team)));
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
            });
        }
        for i in 0..world.seats.len() {
            world.spawn_hero(i);
        }
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
            hero_spawn_pos(seat.team),
        );
        let id = self.units.insert(unit);
        self.seats[seat_index].unit = Some(id);
        self.seats[seat_index].respawn_left = 0;
    }

    /// The seat behind a slot, if the slot is in the match.
    pub fn seat(&self, slot: SlotId) -> Option<&SeatState> {
        self.seats.iter().find(|s| s.slot == slot)
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
        }
        f.finish()
    }
}

/// The fountain position of a team.
pub fn fountain_pos(team: Team) -> Vec2 {
    match team {
        Team::Radiant => rules::RADIANT_FOUNTAIN_POS,
        Team::Dire => rules::DIRE_FOUNTAIN_POS,
    }
}

/// Where a team's hero appears, beside the fountain rather than inside it.
pub fn hero_spawn_pos(team: Team) -> Vec2 {
    let offset = match team {
        Team::Radiant => Vec2::from_ints(rules::HERO_SPAWN_OFFSET, rules::HERO_SPAWN_OFFSET),
        Team::Dire => Vec2::from_ints(-rules::HERO_SPAWN_OFFSET, -rules::HERO_SPAWN_OFFSET),
    };
    fountain_pos(team) + offset
}

/// The Ancient position of a team.
pub fn ancient_pos(team: Team) -> Vec2 {
    match team {
        Team::Radiant => rules::RADIANT_ANCIENT_POS,
        Team::Dire => rules::DIRE_ANCIENT_POS,
    }
}

/// The mid tower position of a team.
pub fn tower_pos(team: Team) -> Vec2 {
    match team {
        Team::Radiant => rules::RADIANT_TOWER_POS,
        Team::Dire => rules::DIRE_TOWER_POS,
    }
}

/// The creep spawn position of a team.
pub fn creep_spawn_pos(team: Team) -> Vec2 {
    match team {
        Team::Radiant => rules::RADIANT_CREEP_SPAWN,
        Team::Dire => rules::DIRE_CREEP_SPAWN,
    }
}

/// The side a team's creeps push towards.
pub fn enemy_of(team: Team) -> Team {
    match team {
        Team::Radiant => Team::Dire,
        Team::Dire => Team::Radiant,
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
    }
}
