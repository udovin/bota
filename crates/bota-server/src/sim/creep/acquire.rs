//! Target acquisition: who a unit picks to attack when nobody told it.

use bota_proto::{EntityId, Fixed, Team, UnitKind};

use crate::sim::{CAMPS, Unit, UnitOrder, World, isqrt64, rules};

/// The class order a unit ranks its targets by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriorityOrder {
    /// Heroes and ordinary units, then siege creeps, then buildings, then
    /// wards. What everything but a siege creep uses.
    Normal,
    /// Buildings, then siege creeps, then everything else, then wards.
    SiegeFirst,
}

/// The priority class of a target, before the class order is applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetClass {
    Unit,
    Siege,
    Building,
    Ward,
}

fn class_of(unit: &Unit) -> TargetClass {
    match unit.kind {
        UnitKind::CreepSiege => TargetClass::Siege,
        UnitKind::Tower | UnitKind::Ancient | UnitKind::Fountain => TargetClass::Building,
        UnitKind::Ward => TargetClass::Ward,
        _ => TargetClass::Unit,
    }
}

fn class_rank(class: TargetClass, order: PriorityOrder) -> u8 {
    match order {
        PriorityOrder::Normal => match class {
            TargetClass::Unit => 0,
            TargetClass::Siege => 1,
            TargetClass::Building => 2,
            TargetClass::Ward => 3,
        },
        PriorityOrder::SiegeFirst => match class {
            TargetClass::Building => 0,
            TargetClass::Siege => 1,
            TargetClass::Unit => 2,
            TargetClass::Ward => 3,
        },
    }
}

/// Whether the camp at this spot is one lane creeps will fight.
///
/// The four camps the map marks with an aggro type of one.
pub fn pullable_camp(pos: bota_proto::Vec2) -> bool {
    CAMPS.iter().any(|c| c.pullable && c.pos == pos)
}

/// Whether `seeker` attacks `target` of its own accord.
///
/// Team alone does not decide it: the jungle is hostile to both sides, but
/// only the pull camps are hostile back to lane creeps, and towers never
/// shoot the jungle at all.
pub fn hostile(seeker: &Unit, target: &Unit) -> bool {
    if target.invulnerable || target.hp <= 0 || seeker.team == target.team {
        return false;
    }
    if target.team == Team::Neutral {
        if seeker.is_structure() {
            return false;
        }
        if seeker.is_creep() {
            return pullable_camp(target.camp);
        }
    }
    true
}

/// Where a hero sits among equally close candidates.
///
/// Zero for one attacking a hero of the seeker's own side, two for one
/// attacking its own allies, one for everything else. Non-heroes are always
/// one, and so is a hero merely last hitting a creep.
fn behaviour_rank(world: &World, seeker_team: Team, candidate: &Unit) -> u8 {
    if candidate.kind != UnitKind::Hero {
        return 1;
    }
    let UnitOrder::Attack { target, .. } = candidate.order else {
        return 1;
    };
    match world.units.get(target) {
        Some(t) if t.team == seeker_team && t.kind == UnitKind::Hero => 0,
        Some(t) if t.team == candidate.team => 2,
        _ => 1,
    }
}

/// The target a unit acquires within `range`, or nothing.
///
/// The best class decides first. Inside it the closest candidate sets the
/// mark, and everything within [`rules::AGGRO_TIE_RANGE`] of that mark counts
/// as equally close; among those, what a hero is doing decides, then the
/// distance itself, then the entity id.
pub fn acquire(
    world: &World,
    id: EntityId,
    range: Fixed,
    order: PriorityOrder,
) -> Option<EntityId> {
    acquire_demoting(world, id, range, order, None)
}

/// The same, with one candidate put last however close it stands.
///
/// An attack order at an ally demotes the hero that gave it: anyone else in
/// range is ranked first, and that hero is taken only when there is nobody
/// else to take.
pub fn acquire_demoting(
    world: &World,
    id: EntityId,
    range: Fixed,
    order: PriorityOrder,
    demoted: Option<EntityId>,
) -> Option<EntityId> {
    ranked(world, id, range, order, demoted)
        .or_else(|| demoted.filter(|&last| reachable(world, id, range, last)))
}

/// Whether a unit is a candidate for another at all: hostile and in range.
fn reachable(world: &World, id: EntityId, range: Fixed, other_id: EntityId) -> bool {
    let (Some(seeker), Some(other)) = (world.units.get(id), world.units.get(other_id)) else {
        return false;
    };
    hostile(seeker, other)
        && seeker
            .pos
            .within(other.pos, range + seeker.radius + other.radius)
}

fn ranked(
    world: &World,
    id: EntityId,
    range: Fixed,
    order: PriorityOrder,
    skip: Option<EntityId>,
) -> Option<EntityId> {
    let seeker = world.units.get(id)?;
    let mut best_class = u8::MAX;
    let mut nearest = i64::MAX;
    let mut found: Vec<(u8, i64, EntityId)> = Vec::new();
    for (other_id, other) in world.units.iter() {
        if other_id == id || Some(other_id) == skip || !hostile(seeker, other) {
            continue;
        }
        let reach = range + seeker.radius + other.radius;
        if !seeker.pos.within(other.pos, reach) {
            continue;
        }
        let class = class_rank(class_of(other), order);
        let distance = isqrt64(seeker.pos.distance_squared(other.pos));
        if class < best_class {
            best_class = class;
            nearest = distance;
        } else if class == best_class && distance < nearest {
            nearest = distance;
        }
        found.push((class, distance, other_id));
    }
    let tie = i64::from(rules::units(rules::AGGRO_TIE_RANGE).raw);
    found
        .into_iter()
        .filter(|&(class, distance, _)| class == best_class && distance <= nearest + tie)
        .min_by_key(|&(_, distance, other_id)| {
            let rank = world
                .units
                .get(other_id)
                .map_or(1, |u| behaviour_rank(world, seeker.team, u));
            (rank, distance, other_id)
        })
        .map(|(_, _, other_id)| other_id)
}

/// The class order a unit ranks targets by.
pub fn priority_of(unit: &Unit) -> PriorityOrder {
    if unit.kind == UnitKind::CreepSiege {
        PriorityOrder::SiegeFirst
    } else {
        PriorityOrder::Normal
    }
}

/// Whether something in range belongs to a better class than the held target.
///
/// A creep chewing on a building drops it for a unit, a siege creep or a ward
/// never takes precedence over one. Distance alone never triggers a switch:
/// walking a hero past a busy creep does not steal it.
pub fn outranked(
    world: &World,
    id: EntityId,
    held: EntityId,
    range: Fixed,
    order: PriorityOrder,
) -> bool {
    let Some(seeker) = world.units.get(id) else {
        return false;
    };
    let Some(held) = world.units.get(held) else {
        return false;
    };
    let holding = class_rank(class_of(held), order);
    world.units.iter().any(|(other_id, other)| {
        other_id != id
            && hostile(seeker, other)
            && seeker
                .pos
                .within(other.pos, range + seeker.radius + other.radius)
            && class_rank(class_of(other), order) < holding
    })
}
