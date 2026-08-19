//! Working out who sees what.

use bota_proto::{Fixed, Team, UnitKind, Vec2};

use crate::engine::{
    Entity, EntityAllocator, Stats, Table, Transform, Visibility, World, is_structure,
};
use crate::sim::{Event, EventVisibility, Ground, PassGrid, sight_clear};

/// Rewrites who sees each entity.
///
/// The row itself belongs to the entity: it is made when the entity is and
/// given up when it goes, so this only ever rewrites what is already there.
///
/// A side always sees its own, and both sides always see a building. Beyond
/// that, everything that can see is walked
/// in turn against everything inside its sight radius, and the line is traced
/// only for what its side cannot already see: once one pair of eyes has an
/// entity, no other pair of the same side pays for it again.
/// What working out sight reads and writes.
///
/// The set runs past what is readable named one by one at a call site;
/// gathered here, the access a system takes is still declared, and still
/// checked when [`World::step`] hands the tables over.
///
/// [`World::step`]: crate::engine::World::step
pub struct SightCx<'a> {
    /// Which entities exist.
    pub entities: &'a EntityAllocator,
    /// Where each entity stands.
    pub transform: &'a Table<Transform>,
    /// Which side each entity is on.
    pub team: &'a Table<Team>,
    /// What kind of thing each entity is.
    pub kind: &'a Table<UnitKind>,
    /// How far each entity sees.
    pub stats: &'a Table<Stats>,
    /// The height of the ground.
    pub ground: &'a Ground,
    /// Which cells stop a sight line.
    pub sight_block: &'a PassGrid,
    /// Where the answer goes.
    pub visibility: &'a mut Table<Visibility>,
}

pub fn visibility_system(cx: SightCx<'_>) {
    let SightCx {
        entities,
        transform,
        team,
        kind,
        stats,
        ground,
        sight_block,
        visibility,
    } = cx;

    for entity in entities.iter() {
        let Some(seen) = visibility.get_mut(entity) else {
            continue;
        };
        seen.clear();
        if let Some(side) = team.get(entity).copied() {
            seen.add(side);
        }
        // A building is on every map both sides look at, ward and unit alike
        // are not.
        if kind.get(entity).copied().is_some_and(is_structure) {
            seen.add(Team::Radiant);
            seen.add(Team::Dire);
        }
    }
    for viewer in entities.iter() {
        let (Some(side), Some(from), Some(radius)) = (
            team.get(viewer).copied(),
            transform.get(viewer).map(|t| t.pos),
            stats.get(viewer).map(|s| s.vision),
        ) else {
            continue;
        };
        if radius <= Fixed::ZERO {
            continue;
        }
        let viewer_tier = ground.tier(from);
        for target in entities.iter() {
            if visibility.get(target).is_some_and(|seen| seen.by(side)) {
                continue;
            }
            let Some(at) = transform.get(target).map(|t| t.pos) else {
                continue;
            };
            if !from.within(at, radius) || viewer_tier < ground.tier(at) {
                continue;
            }
            if sight_clear(ground, sight_block, from, viewer_tier, at)
                && let Some(seen) = visibility.get_mut(target)
            {
                seen.add(side);
            }
        }
    }
}

impl World {
    /// Whether a side sees a point on the map.
    ///
    /// A point has no row of its own, so this is asked live: for an event at a
    /// spot, or for an order at somewhere nobody stands.
    pub fn can_see_point(&self, team: Team, at: Vec2) -> bool {
        let target_tier = self.ground.tier(at);
        self.entities.iter().any(|entity| {
            let (Some(side), Some(from), Some(radius)) = (
                self.team.get(entity).copied(),
                self.transform.get(entity).map(|t| t.pos),
                self.stats.get(entity).map(|s| s.vision),
            ) else {
                return false;
            };
            let viewer_tier = self.ground.tier(from);
            side == team
                && radius > Fixed::ZERO
                && from.within(at, radius)
                && viewer_tier >= target_tier
                && sight_clear(&self.ground, &self.sight_block, from, viewer_tier, at)
        })
    }

    /// Whether a side sees an entity, from what was worked out this tick.
    pub fn can_see(&self, team: Team, entity: Entity) -> bool {
        self.visibility
            .get(entity)
            .is_some_and(|seen| seen.by(team))
    }

    /// Who may learn of something that happened at a point, when one side is
    /// party to it.
    ///
    /// A side is told of what it can see, and always of what involves it.
    pub fn who_may_know(&self, at: Vec2, involved: Team) -> EventVisibility {
        let radiant = involved == Team::Radiant || self.can_see_point(Team::Radiant, at);
        let dire = involved == Team::Dire || self.can_see_point(Team::Dire, at);
        match (radiant, dire) {
            (true, true) => EventVisibility::Everyone,
            (true, false) => EventVisibility::OneTeam(Team::Radiant),
            (false, true) => EventVisibility::OneTeam(Team::Dire),
            (false, false) => EventVisibility::OneTeam(involved),
        }
    }

    /// Keeps from a side what it had no way of seeing.
    pub fn hide_unseen(&self, events: &mut [Event], places: &[(usize, Vec2, Team)]) {
        for &(index, at, involved) in places {
            if let Some(event) = events.get_mut(index) {
                event.visible_to = self.who_may_know(at, involved);
            }
        }
    }
}
