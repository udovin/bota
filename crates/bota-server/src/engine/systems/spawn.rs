//! Putting an entity into the world with everything its kind needs.

use bota_proto::{Angle, Fixed, HeroId, SlotId, Team, UnitKind, Vec2};

use crate::engine::{
    Attacking, Auras, Bounty, Def, Entity, Health, Hull, Level, Mana, March, Orders, Transform,
    UnitOrder, World,
};

impl World {
    /// Puts one unit on the map and gives it what its kind carries.
    ///
    /// Health and mana start empty; the system that works out stats fills them
    /// on the tick after, since the maximum is its to decide.
    pub fn spawn_unit(
        &mut self,
        def: &'static crate::engine::UnitDef,
        team: Team,
        pos: Vec2,
    ) -> Entity {
        let entity = self.spawn();
        self.def.insert(entity, Def(def));
        self.kind.insert(entity, def.kind);
        self.set_team(entity, team);
        self.transform.insert(
            entity,
            Transform {
                pos,
                facing: Angle::default(),
            },
        );
        // A kind that takes no room on the ground gets no hull at all, and so
        // is passed through rather than walked round or eased apart.
        if def.radius > 0 {
            self.hull.insert(
                entity,
                Hull {
                    radius: Fixed::from_int(def.radius),
                },
            );
        }
        self.health.insert(entity, Health { hp: Fixed::ZERO });
        if !def.auras.is_empty() {
            self.auras.insert(entity, Auras(def.auras));
        }
        if def.max_mana > 0 {
            self.mana.insert(entity, Mana { mana: Fixed::ZERO });
        }
        if def.bounty_gold > 0 || def.bounty_xp > 0 {
            self.bounty.insert(
                entity,
                Bounty {
                    gold: def.bounty_gold,
                    xp: def.bounty_xp,
                },
            );
        }
        if def.damage > 0 {
            self.attacking.insert(
                entity,
                Attacking {
                    windup: None,
                    cooldown: 0,
                    recovering: 0,
                },
            );
        }
        if def.move_speed > 0 {
            self.orders.insert(
                entity,
                Orders {
                    current: UnitOrder::Idle,
                    cooldown: 0,
                },
            );
        }
        if matches!(
            def.kind,
            UnitKind::CreepMelee
                | UnitKind::CreepFlagbearer
                | UnitKind::CreepRanged
                | UnitKind::CreepSiege
        ) {
            self.march.insert(
                entity,
                March {
                    route_step: 0,
                    trace: None,
                    shove: 0,
                },
            );
        }
        entity
    }

    /// Puts a hero on the map for a seat.
    pub fn spawn_hero(&mut self, team: Team, pos: Vec2, slot: SlotId, hero: HeroId) -> Entity {
        let entity = self.spawn_unit(&crate::engine::HERO, team, pos);
        self.owner.insert(entity, slot);
        self.hero.insert(entity, hero);
        self.level.insert(entity, Level(1));
        self.abilities.insert(entity, crate::engine::hero_kit());
        self.inventory.insert(
            entity,
            crate::engine::Inventory::empty(
                crate::sim::rules::INVENTORY_SLOTS + crate::sim::rules::BACKPACK_SLOTS,
            ),
        );
        entity
    }

    /// Fills a unit's pools to whatever its stats now allow.
    ///
    /// Called once the stats are known, which is why spawning does not do it.
    pub fn fill_pools(&mut self, entity: Entity) {
        let Some(stats) = self.stats.get(entity).copied() else {
            return;
        };
        if let Some(health) = self.health.get_mut(entity) {
            health.hp = stats.max_hp;
        }
        if let Some(mana) = self.mana.get_mut(entity) {
            mana.mana = stats.max_mana;
        }
    }
}
