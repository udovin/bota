//! The world: one table per component, and the order a tick runs in.

use bota_proto::{HeroId, SlotId, Team, UnitKind};

use crate::engine::{
    AbilityBook, Attacking, Bounty, CampHome, Def, Entity, EntityAllocator, Health, Hull,
    Inventory, Lane, LaneAi, Level, Mana, March, NeutralAi, Orders, PendingCast, Projectile, Route,
    Seat, SightCx, Stats, StatsCx, Statuses, Table, Tier, Transform, Upgrades, Visibility,
    derive_stats, regenerate, visibility_system,
};

/// Everything a match is made of.
///
/// Every component lives in a table of its own, and a table is absent for an
/// entity the component does not apply to. A system takes the tables it needs
/// and nothing else; [`World::step`] is the one place the order of a tick is
/// written down.
pub struct World {
    /// Ticks since the match began.
    pub tick: u32,
    /// A place per player.
    pub seats: Vec<Seat>,
    /// The side that has won, once one has.
    pub winner: Option<Team>,
    /// The map it is played on.
    pub map: &'static crate::sim::MapDef,
    /// Where every roll of the dice comes from.
    pub rng: crate::sim::MatchRng,
    /// Which cells anything may stand on.
    pub grid: crate::sim::PassGrid,
    /// Which roster each camp put out last, so it never draws twice running.
    pub camp_last: Vec<u8>,
    /// The height of the ground everywhere.
    pub ground: crate::sim::Ground,
    /// Which cells stop a sight line: trees and the map's own walls.
    pub sight_block: crate::sim::PassGrid,
    /// Which entities exist.
    pub entities: EntityAllocator,

    /// Where each entity stands.
    pub transform: Table<Transform>,
    /// The room each entity takes.
    pub hull: Table<Hull>,
    /// What kind of thing each entity is.
    pub kind: Table<UnitKind>,
    /// Which side each entity is on.
    pub team: Table<Team>,

    /// Health, for whatever can be hurt.
    pub health: Table<Health>,
    /// Mana, for whatever spends it.
    pub mana: Table<Mana>,

    /// Which kind of unit each entity is.
    pub def: Table<Def>,
    /// Hero levels.
    pub level: Table<Level>,
    /// Upgrade intervals a creep spawned after.
    pub upgrades: Table<Upgrades>,
    /// Building tiers.
    pub tier: Table<Tier>,
    /// The numbers each entity fights by, worked out afresh every tick.
    pub stats: Table<Stats>,
    /// What is on each entity and runs out on its own.
    pub statuses: Table<Statuses>,

    /// Routes being walked by whoever a player drives.
    pub route: Table<Route>,
    /// What a creep keeps while marching its lane.
    pub march: Table<March>,

    /// The order each entity is following.
    pub orders: Table<Orders>,
    /// Who each entity is set on. Absent when it is set on nobody.
    pub engage: Table<Entity>,
    /// Where each entity is in its attack cycle.
    pub attacking: Table<Attacking>,
    /// Casts ordered and not yet started.
    pub casting: Table<PendingCast>,

    /// What a lane creep keeps about the fight it is in.
    pub lane_ai: Table<LaneAi>,
    /// What a neutral keeps about being drawn away.
    pub neutral_ai: Table<NeutralAi>,
    /// Which camp a neutral belongs to.
    pub camp_home: Table<CampHome>,

    /// Which lane an entity belongs to.
    pub lane: Table<Lane>,
    /// What killing an entity pays.
    pub bounty: Table<Bounty>,
    /// Which seat owns an entity.
    pub owner: Table<SlotId>,
    /// Which hero a hero entity is.
    pub hero: Table<HeroId>,
    /// What each entity carries.
    pub inventory: Table<Inventory>,
    /// What each entity can cast.
    pub abilities: Table<AbilityBook>,

    /// Which sides see each entity, worked out afresh every tick.
    pub visibility: Table<Visibility>,

    /// Missiles in flight.
    pub projectile: Table<Projectile>,
}

impl Default for World {
    fn default() -> Self {
        World::new()
    }
}

impl World {
    /// A world at tick zero with nothing in it, on the Dota map.
    pub fn new() -> World {
        World {
            tick: 0,
            seats: Vec::new(),
            winner: None,
            map: crate::sim::map_of(bota_proto::MapId(0)),
            rng: crate::sim::MatchRng::new(&[0; 32], 0),
            grid: crate::sim::PassGrid::open(),
            camp_last: Vec::new(),
            ground: crate::sim::Ground::of(crate::sim::map_of(bota_proto::MapId(0))),
            sight_block: crate::sim::PassGrid::open(),
            entities: EntityAllocator::new(),
            transform: Table::new(),
            hull: Table::new(),
            kind: Table::new(),
            team: Table::new(),
            health: Table::new(),
            mana: Table::new(),
            def: Table::new(),
            level: Table::new(),
            upgrades: Table::new(),
            tier: Table::new(),
            stats: Table::new(),
            statuses: Table::new(),
            route: Table::new(),
            march: Table::new(),
            orders: Table::new(),
            engage: Table::new(),
            attacking: Table::new(),
            casting: Table::new(),
            lane_ai: Table::new(),
            neutral_ai: Table::new(),
            camp_home: Table::new(),
            lane: Table::new(),
            bounty: Table::new(),
            owner: Table::new(),
            hero: Table::new(),
            inventory: Table::new(),
            abilities: Table::new(),
            visibility: Table::new(),
            projectile: Table::new(),
        }
    }

    /// Adds an entity carrying no components.
    pub fn spawn(&mut self) -> Entity {
        self.entities.alloc()
    }

    /// Puts an entity on a side.
    ///
    /// Standing on a side is what makes an entity something sides can see, so
    /// its row in the sight table is made here and nowhere else. That side has
    /// it from this moment rather than from the next pass of sight, which
    /// matters for whatever is stood up mid-tick.
    pub fn set_team(&mut self, entity: Entity, team: Team) {
        self.team.insert(entity, team);
        match self.visibility.get_mut(entity) {
            Some(seen) => seen.add(team),
            None => {
                let mut seen = Visibility::NONE;
                seen.add(team);
                self.visibility.insert(entity, seen);
            }
        }
    }

    /// Takes an entity out of the world. False when the handle named nobody
    /// live.
    ///
    /// What sides could see of it is given up here. What it held besides stays
    /// where it is; the slot's next tenant carries a raised generation, so none
    /// of it reads back as that tenant's own.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        self.visibility.remove(entity);
        self.entities.free(entity)
    }

    /// A world with a map's buildings standing and their pools full.
    pub fn on_map(map: &'static crate::sim::MapDef) -> World {
        let mut world = World::new();
        world.map = map;
        world.grid = crate::sim::build_grid(map);
        world.ground = crate::sim::Ground::of(map);
        world.sight_block = crate::sim::build_sight_block(map);
        for (index, team) in [Team::Radiant, Team::Dire].into_iter().enumerate() {
            world.spawn_unit(&crate::engine::FOUNTAIN, team, map.fountains[index]);
            world.spawn_unit(&crate::engine::ANCIENT, team, map.ancients[index]);
            let towers = if index == 0 {
                map.radiant_towers
            } else {
                map.dire_towers
            };
            for (lane, tier, pos) in towers {
                let entity = world.spawn_unit(crate::engine::tower_def(*tier), team, *pos);
                world.lane.insert(entity, Lane(*lane));
                world.tier.insert(entity, Tier(*tier));
            }
        }
        world.settle();
        world
    }

    /// Brings everything worked out into line with what stands: stats, pools
    /// and who sees what.
    ///
    /// Called for a world just built and after anything is put into one, so
    /// nothing newly stood up is invisible until the next tick.
    pub fn settle(&mut self) {
        derive_stats(StatsCx {
            entities: &self.entities,
            def: &self.def,
            level: &self.level,
            upgrades: &self.upgrades,
            inventory: &self.inventory,
            statuses: &self.statuses,
            stats: &mut self.stats,
            health: &mut self.health,
            mana: &mut self.mana,
        });
        for entity in self.entities.iter().collect::<Vec<_>>() {
            self.fill_pools(entity);
        }
        visibility_system(SightCx {
            entities: &self.entities,
            transform: &self.transform,
            team: &self.team,
            kind: &self.kind,
            stats: &self.stats,
            ground: &self.ground,
            sight_block: &self.sight_block,
            visibility: &mut self.visibility,
        });
    }

    /// One tick. Systems run in the order they are written here.
    pub fn step(&mut self) -> Vec<crate::sim::Event> {
        let mut events = Vec::new();
        let mut hits = Vec::new();
        self.tick += 1;
        self.spawn_waves();
        self.fill_camps();
        self.tick_gear();
        self.passive_gold();
        self.tick_respawns();
        derive_stats(StatsCx {
            entities: &self.entities,
            def: &self.def,
            level: &self.level,
            upgrades: &self.upgrades,
            inventory: &self.inventory,
            statuses: &self.statuses,
            stats: &mut self.stats,
            health: &mut self.health,
            mana: &mut self.mana,
        });
        self.tick_lane_ai();
        self.acquire_targets();
        self.tick_jungle();
        self.march_lanes();
        self.walk_bodies();
        self.push_apart();
        visibility_system(SightCx {
            entities: &self.entities,
            transform: &self.transform,
            team: &self.team,
            kind: &self.kind,
            stats: &self.stats,
            ground: &self.ground,
            sight_block: &self.sight_block,
            visibility: &mut self.visibility,
        });
        regenerate(
            &self.entities,
            &self.stats,
            &mut self.health,
            &mut self.mana,
        );
        self.swing(&mut hits);
        self.fly(&mut hits);
        self.run_casts(&mut events, &mut hits);
        let fallen = self.resolve(hits, &mut events);
        self.bury(fallen, &mut events);
        events
    }
}
