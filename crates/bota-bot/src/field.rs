//! One tick, read into a fixed shape.
//!
//! Everything a snapshot holds that the model is allowed to care about, put in
//! a settled order and turned about so that it reads the same from either end
//! of the map. Both halves of the contract are built from this and from
//! nothing else: the numbers the model is shown, and what a chosen deed means.
//! That is the point of it being one place — the third creep in the vector and
//! the third creep in "swing at the third creep" have to be the same creep, or
//! everything above is learning noise.
//!
//! Turned about, because the two sides of the map are mirrors. In world
//! coordinates a bot would have to learn the game twice, once from each corner.
//! Here forward is always towards the other side's fountain and left is always
//! left of that, so one set of weights serves both.

use bota_proto::{EntityId, PlayerView, SlotId, Team, UnitKind, UnitView, Vec2, WorldView};

use crate::{Lane, Role, lane_of};

/// Creeps of the other side the model is shown and may name.
pub const CREEPS: usize = 5;
/// Creeps of its own, for putting out.
pub const OWN_CREEPS: usize = 3;
/// Heroes of the other side.
pub const HEROES: usize = 5;

/// One tick as the model sees it.
pub struct Field<'a> {
    /// The snapshot it was read from.
    pub view: &'a WorldView,
    /// The seat being played.
    pub seat: &'a PlayerView,
    /// The body it drives, while one is standing.
    pub me: Option<&'a UnitView>,
    /// The side it plays for.
    pub team: Team,
    /// Which way is forward, as a unit vector in world coordinates.
    pub forward: (f32, f32),
    /// Creeps of the other side, nearest first.
    pub creeps: Vec<&'a UnitView>,
    /// Creeps of its own, nearest first.
    pub own_creeps: Vec<&'a UnitView>,
    /// Heroes of the other side, by seat so that the order never shifts.
    pub heroes: Vec<&'a UnitView>,
    /// Its own courier, while one is standing.
    pub courier: Option<&'a UnitView>,
    /// The nearest tower of each side: its own, then the other's.
    pub towers: (Option<&'a UnitView>, Option<&'a UnitView>),
    /// Where its own side's fountain stands.
    pub home: Option<Vec2>,
    /// Where the other side's stands.
    pub away: Option<Vec2>,
    /// What this seat is there to do.
    pub role: Role,
}

impl<'a> Field<'a> {
    /// Reads one tick for one seat, playing the role given.
    pub fn of(view: &'a WorldView, slot: SlotId, role: Role) -> Option<Field<'a>> {
        let seat = view.players.iter().find(|player| player.slot == slot)?;
        let team = seat.team;
        let me = seat
            .unit
            .and_then(|id| view.units.iter().find(|unit| unit.id == id));
        let home = fountain(view, team);
        let away = fountain(view, other_side(team));
        let forward = match (home, away) {
            (Some(home), Some(away)) => unit_vector(home, away),
            _ => (1.0, 0.0),
        };
        let at = me.map_or(home.unwrap_or(Vec2::ZERO), |unit| unit.pos);
        let standing = |unit: &&UnitView| unit.hp > 0;

        let mut creeps: Vec<&UnitView> = view
            .units
            .iter()
            .filter(standing)
            .filter(|unit| unit.team != team && unit.team != Team::Neutral)
            .filter(|unit| is_wave_creep(unit.kind))
            .collect();
        rank_by_nearness(&mut creeps, at);
        creeps.truncate(CREEPS);

        let mut own_creeps: Vec<&UnitView> = view
            .units
            .iter()
            .filter(standing)
            .filter(|unit| unit.team == team && is_wave_creep(unit.kind))
            .collect();
        rank_by_nearness(&mut own_creeps, at);
        own_creeps.truncate(OWN_CREEPS);

        // Heroes are ordered by the seat they belong to rather than by where
        // they stand: a hero that walks past another must not swap places in
        // the list, or the deed that named one would name the other.
        let mut heroes: Vec<&UnitView> = view
            .units
            .iter()
            .filter(standing)
            .filter(|unit| unit.team != team && unit.kind == UnitKind::Hero)
            .collect();
        heroes.sort_by_key(|unit| unit.owner.map_or(u8::MAX, |slot| slot.0));
        heroes.truncate(HEROES);

        let courier = view
            .units
            .iter()
            .filter(standing)
            .find(|unit| unit.kind == UnitKind::Courier && unit.owner == Some(slot));

        let nearest_tower = |side: Team| {
            let mut towers: Vec<&UnitView> = view
                .units
                .iter()
                .filter(standing)
                .filter(|unit| unit.team == side && unit.kind == UnitKind::Tower)
                .collect();
            rank_by_nearness(&mut towers, at);
            towers.first().copied()
        };

        Some(Field {
            view,
            seat,
            me,
            team,
            forward,
            creeps,
            own_creeps,
            heroes,
            courier,
            towers: (nearest_tower(team), nearest_tower(other_side(team))),
            home,
            away,
            role,
        })
    }

    /// Where the bot stands, or its own fountain while it stands nowhere.
    pub fn at(&self) -> Vec2 {
        self.me
            .map(|unit| unit.pos)
            .or(self.home)
            .unwrap_or(Vec2::ZERO)
    }

    /// A spot as the model sees it: forward and left of the bot, in thousands
    /// of world units.
    ///
    /// Thousands because a lane is some fifteen of them across and a swing
    /// reaches half of one, so everything the model weighs lands within a few
    /// either way.
    pub fn seen_from_here(&self, spot: Vec2) -> (f32, f32) {
        let at = self.at();
        let (dx, dy) = (
            spot.x.to_f32() - at.x.to_f32(),
            spot.y.to_f32() - at.y.to_f32(),
        );
        let (fx, fy) = self.forward;
        ((dx * fx + dy * fy) / 1000.0, (-dx * fy + dy * fx) / 1000.0)
    }

    /// A spot that many world units forward and left of where the bot stands.
    pub fn spot_towards(&self, forward: f32, left: f32) -> Vec2 {
        let at = self.at();
        let (fx, fy) = self.forward;
        let x = at.x.to_f32() + forward * fx - left * fy;
        let y = at.y.to_f32() + forward * fy + left * fx;
        Vec2::from_ints(
            x.clamp(0.0, MAP_SIZE).round() as i32,
            y.clamp(0.0, MAP_SIZE).round() as i32,
        )
    }

    /// The ground between the bot and a body, edge to edge.
    pub fn gap_to(&self, other: &UnitView) -> f32 {
        let Some(me) = self.me else {
            return f32::MAX;
        };
        span(me.pos, other.pos) - me.radius.to_f32() - other.radius.to_f32()
    }

    /// Whether a body stands within a swing.
    pub fn in_reach(&self, other: &UnitView) -> bool {
        self.me
            .is_some_and(|me| self.gap_to(other) <= me.attack_range.to_f32())
    }

    /// Whether the bot has a body standing at all.
    pub fn alive(&self) -> bool {
        self.me.is_some()
    }

    /// The side that is not its own.
    pub fn other_side(&self) -> Team {
        other_side(self.team)
    }
}

/// The world spans this many units on each axis.
pub const MAP_SIZE: f32 = 18432.0;

/// The side that is not this one.
pub fn other_side(team: Team) -> Team {
    match team {
        Team::Radiant => Team::Dire,
        Team::Dire => Team::Radiant,
        Team::Neutral => Team::Neutral,
    }
}

/// Whether a kind is one of the creeps a lane wave is made of.
pub fn is_wave_creep(kind: UnitKind) -> bool {
    matches!(
        kind,
        UnitKind::CreepMelee
            | UnitKind::CreepFlagbearer
            | UnitKind::CreepRanged
            | UnitKind::CreepSiege
    )
}

/// Where a side's fountain stands. Both sides always see every building.
fn fountain(view: &WorldView, team: Team) -> Option<Vec2> {
    view.units
        .iter()
        .find(|unit| unit.team == team && unit.kind == UnitKind::Fountain)
        .map(|unit| unit.pos)
}

/// Puts bodies in order of nearness, with the handle breaking ties.
///
/// The tie-break matters more than it looks: two creeps the same distance off
/// would otherwise swap places from tick to tick, and the deed that named one
/// would name the other.
fn rank_by_nearness(bodies: &mut [&UnitView], at: Vec2) {
    bodies.sort_by(|one, other| {
        span(at, one.pos)
            .partial_cmp(&span(at, other.pos))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(one.id.idx.cmp(&other.id.idx))
    });
}

/// The direction from one spot to another, as a unit vector.
fn unit_vector(from: Vec2, to: Vec2) -> (f32, f32) {
    let (dx, dy) = (
        to.x.to_f32() - from.x.to_f32(),
        to.y.to_f32() - from.y.to_f32(),
    );
    let length = (dx * dx + dy * dy).sqrt();
    if length <= f32::EPSILON {
        (1.0, 0.0)
    } else {
        (dx / length, dy / length)
    }
}

/// How far apart two spots are.
pub fn span(one: Vec2, other: Vec2) -> f32 {
    let (dx, dy) = (
        one.x.to_f32() - other.x.to_f32(),
        one.y.to_f32() - other.y.to_f32(),
    );
    (dx * dx + dy * dy).sqrt()
}

/// One number as a part of another, and zero when there is no whole.
pub fn part(some: i32, whole: i32) -> f32 {
    if whole <= 0 {
        return 0.0;
    }
    some.max(0) as f32 / whole as f32
}

/// The handle of a body, for naming it in an order.
pub fn handle(unit: &UnitView) -> EntityId {
    unit.id
}

impl Field<'_> {
    /// The lane this seat is there to hold.
    pub fn lane(&self) -> Option<Lane> {
        lane_of(self, self.role)
    }
}
