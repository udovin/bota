//! A fingerprint of the world, for telling two runs apart.
//!
//! Everything the simulation acts on goes in, walked in slot order and never
//! through a hash map, so the same run always gives the same number.

use bota_proto::{Angle, Fixed, Team, UnitKind, Vec2};

use crate::engine::{Entity, Inventory, StatusKind, Target, World};

/// FNV-1a over the state a tick depends on.
pub struct Fnv {
    /// What has been eaten so far.
    state: u64,
}

impl Default for Fnv {
    fn default() -> Self {
        Fnv::new()
    }
}

impl Fnv {
    /// A fingerprint of nothing.
    pub fn new() -> Fnv {
        Fnv {
            state: 0xcbf2_9ce4_8422_2325,
        }
    }

    /// Eats one byte.
    pub fn u8(&mut self, value: u8) {
        self.state ^= u64::from(value);
        self.state = self.state.wrapping_mul(0x0000_0100_0000_01b3);
    }

    /// Eats four bytes, low first.
    pub fn u32(&mut self, value: u32) {
        for byte in value.to_le_bytes() {
            self.u8(byte);
        }
    }

    /// Eats a signed four bytes.
    pub fn i32(&mut self, value: i32) {
        self.u32(value as u32);
    }

    /// Eats a fixed-point number.
    pub fn fixed(&mut self, value: Fixed) {
        self.i32(value.raw);
    }

    /// Eats a position.
    pub fn vec2(&mut self, value: Vec2) {
        self.fixed(value.x);
        self.fixed(value.y);
    }

    /// Eats a facing.
    pub fn angle(&mut self, value: Angle) {
        self.u32(u32::from(value.brads));
    }

    /// Eats a side.
    pub fn team(&mut self, value: Team) {
        self.u8(match value {
            Team::Radiant => 0,
            Team::Dire => 1,
            Team::Neutral => 2,
        });
    }

    /// Eats an entity handle.
    pub fn entity(&mut self, value: Entity) {
        self.u32(value.index().0);
        self.u32(value.generation().0.get());
    }

    /// Eats a kind of unit.
    pub fn kind(&mut self, value: UnitKind) {
        self.u8(value as u8);
    }

    /// Eats what is present or not.
    pub fn some(&mut self, present: bool) {
        self.u8(u8::from(present));
    }

    /// The number so far.
    pub fn done(&self) -> u64 {
        self.state
    }
}

impl World {
    /// A fingerprint of everything a tick acts on.
    ///
    /// Two worlds that agree here have agreed on every position, pool, order
    /// and timer; two that differ have diverged somewhere.
    pub fn hash(&self) -> u64 {
        let mut fnv = Fnv::new();
        fnv.u32(self.tick);
        fnv.some(self.winner.is_some());
        if let Some(winner) = self.winner {
            fnv.team(winner);
        }
        for entity in self.entities.iter() {
            fnv.entity(entity);
            if let Some(kind) = self.kind.get(entity) {
                fnv.kind(*kind);
            }
            if let Some(side) = self.team.get(entity) {
                fnv.team(*side);
            }
            if let Some(at) = self.transform.get(entity) {
                fnv.vec2(at.pos);
                fnv.angle(at.facing);
            }
            if let Some(health) = self.health.get(entity) {
                fnv.fixed(health.hp);
            }
            if let Some(mana) = self.mana.get(entity) {
                fnv.fixed(mana.mana);
            }
            if let Some(Target(on)) = self.target.get(entity) {
                fnv.entity(*on);
            }
            if let Some(attacking) = self.attacking.get(entity) {
                fnv.u32(attacking.cooldown);
                fnv.u32(attacking.recovering);
                fnv.some(attacking.windup.is_some());
                if let Some(windup) = attacking.windup {
                    fnv.entity(windup.target);
                    fnv.u32(windup.ticks_left);
                }
            }
            if let Some(march) = self.march.get(entity) {
                fnv.u32(u32::from(march.route_step));
                fnv.u32(march.shove);
            }
            if let Some(shot) = self.projectile.get(entity) {
                fnv.entity(shot.target);
                fnv.i32(shot.damage);
            }
            if let Some(seen) = self.visibility.get(entity) {
                fnv.u8(seen.bits());
            }
            if let Some(statuses) = self.statuses.get(entity) {
                for status in statuses.0.iter() {
                    hash_status_kind(&mut fnv, status.kind);
                    fnv.u32(status.ticks_left);
                }
            }
            if let Some(bag) = self.inventory.get(entity) {
                hash_bag(&mut fnv, bag);
            }
            if let Some(going) = self.teleport.get(entity) {
                fnv.u32(going.ticks_left);
                fnv.vec2(going.to);
                fnv.u32(going.slot as u32);
            }
        }
        for seat in self.seats.iter() {
            fnv.u32(u32::from(seat.slot.0));
            fnv.i32(seat.gold);
            fnv.i32(seat.xp);
            fnv.u8(seat.level);
            fnv.u32(seat.respawn_left);
            fnv.u32(u32::from(seat.kills));
            fnv.u32(u32::from(seat.deaths));
            fnv.u32(u32::from(seat.last_hits));
            fnv.u32(u32::from(seat.denies));
            hash_bag(&mut fnv, &seat.stash);
        }
        fnv.done()
    }
}

/// One effect kind and everything it carries.
fn hash_status_kind(fnv: &mut Fnv, kind: StatusKind) {
    match kind {
        StatusKind::Haste { pct } => {
            fnv.u8(0);
            fnv.i32(pct);
        }
        StatusKind::Mending { per_tick } => {
            fnv.u8(1);
            fnv.i32(per_tick);
        }
        StatusKind::Clarity { per_tick } => {
            fnv.u8(2);
            fnv.i32(per_tick);
        }
        StatusKind::Fountain {
            hp_per_tick,
            mana_per_tick,
        } => {
            fnv.u8(3);
            fnv.i32(hp_per_tick);
            fnv.i32(mana_per_tick);
        }
    }
}

/// Every slot of a bag, empty ones counted so slots keep their numbers.
fn hash_bag(fnv: &mut Fnv, bag: &Inventory) {
    for slot in bag.slots.iter() {
        match slot {
            None => fnv.u8(0),
            Some(stack) => {
                fnv.u8(1);
                fnv.u32(u32::from(stack.id.0));
                fnv.u8(stack.charges);
                fnv.u32(stack.cooldown);
                fnv.u32(stack.mute);
                fnv.u32(stack.bought_tick);
                fnv.u8(u8::from(stack.touched));
            }
        }
    }
}
