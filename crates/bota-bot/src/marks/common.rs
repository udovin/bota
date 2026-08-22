//! The pieces the lessons are built out of.
//!
//! Shapes and lookups, with no weights of their own and no knowledge of any
//! lesson: nothing here branches on which lesson is asking. What a lesson pays
//! is the lesson's own file; how far off is far, and which creep is the far one,
//! are here because writing them out six times would be six places to get them
//! wrong.

use bota_proto::{EntityId, EventKind, Team, UnitKind, UnitView};

use crate::{Field, Lane, Moment, Role};

/// The distance at which standing near a thing is worth half marks.
const NEAR: f32 = 600.0;
/// The most ground a hero covers in a tick, as far as marking cares.
///
/// The spot being closed on can change from one tick to the next, and the
/// ground that appears to open or close when it does is not ground anybody
/// walked.
const MOST_IN_A_TICK: f32 = 40.0;
/// How far off the line a creep may be and still count as being on the lane.
const ON_THE_LANE: f32 = 3000.0;

/// How much of full marks standing that far off a spot is worth.
///
/// One at the spot itself, a half at [`NEAR`], and falling away from there
/// without ever reaching nothing. There is no distance at which a step towards
/// the spot is worth the same as a step away, however far off it has got: a
/// flat floor leaves a bot that has wandered away with nothing in the numbers
/// pointing home.
pub fn nearness(how_far: f32) -> f32 {
    NEAR / (NEAR + how_far.max(0.0))
}

/// How much ground was closed on a spot since the tick before.
///
/// Ground closed rather than ground left, because a spot most of a lane away is
/// worth about as little to stand near as one the whole lane away, and a policy
/// that has never been there has nothing telling it which way to set off.
/// Closing pays from the first step.
///
/// Nothing on the first tick, when there is nothing to compare against, and
/// never more than a hero could have walked: the spot can change from one tick
/// to the next, and the ground that seems to open when it does is nobody's
/// walking.
pub fn closed(was: Option<f32>, now: f32) -> f32 {
    let Some(was) = was.filter(|was| was.is_finite()) else {
        return 0.0;
    };
    if !now.is_finite() {
        return 0.0;
    }
    (was - now).clamp(-MOST_IN_A_TICK, MOST_IN_A_TICK)
}

/// How much of its health and mana the hero still has, as a share of both.
pub fn wholeness(field: &Field) -> f32 {
    let Some(me) = field.me else {
        return 0.0;
    };
    let share = |now: i32, most: i32| {
        if most <= 0 {
            0.0
        } else {
            (now.max(0) as f32 / most as f32).clamp(0.0, 1.0)
        }
    };
    (share(me.hp, me.max_hp) + share(me.mana, me.max_mana)) / 2.0
}

/// How many blows the hero landed on the other side during the tick.
///
/// On the other side only. Counted for the swing alone, a lesson that pays for
/// blows is answered by cutting down its own wave, which is a thing the game
/// allows and no lesson here is asking for. Something the seat cannot see
/// counts as neither: whose it was is not known.
pub fn blows_landed(now: &Moment) -> usize {
    let Some(me) = now.field.me else {
        return 0;
    };
    now.events
        .iter()
        .filter(|event| match event {
            EventKind::Damaged { source, target, .. } => {
                *source == Some(me.id) && belongs_to_them(now.field, *target)
            }
            _ => false,
        })
        .count()
}

/// Whether the unit named belongs to the other side.
pub fn belongs_to_them(field: &Field, unit: EntityId) -> bool {
    field
        .view
        .units
        .iter()
        .find(|had| had.id == unit)
        .is_some_and(|had| had.team != field.team)
}

/// Whether the unit named is a tower of theirs.
pub fn is_a_tower_of_theirs(field: &Field, unit: EntityId) -> bool {
    field
        .view
        .units
        .iter()
        .find(|had| had.id == unit)
        .is_some_and(|had| had.kind == UnitKind::Tower && had.team != field.team)
}

/// Their nearest tower still standing.
pub fn nearest_tower_of_theirs<'a>(field: &Field<'a>) -> Option<&'a UnitView> {
    let at = field.at();
    field
        .view
        .units
        .iter()
        .filter(|unit| unit.kind == UnitKind::Tower && unit.team != field.team)
        .min_by(|one, other| {
            crate::span(at, one.pos)
                .partial_cmp(&crate::span(at, other.pos))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(one.id.idx.cmp(&other.id.idx))
        })
}

/// The creep of its own that has come furthest up the lane.
///
/// The far end of its own wave, which is where a lane is held from — not the
/// nearest one, which is behind it and usually walking the other way.
pub fn furthest_own_creep<'a>(field: &Field<'a>, lane: &Lane) -> Option<&'a UnitView> {
    field
        .own_creeps
        .iter()
        .copied()
        .filter(|creep| lane.off_the_line(creep.pos) <= ON_THE_LANE)
        .max_by(|one, other| {
            lane.how_far_along(one.pos)
                .partial_cmp(&lane.how_far_along(other.pos))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// Which lane a seat of this role and side is being taught to hold.
pub fn lane_of(field: &Field, role: Role) -> Option<Lane> {
    let home = field.home?;
    let away = field.away?;
    let (radiant, dire) = if field.team == Team::Radiant {
        (home, away)
    } else {
        (away, home)
    };
    Some(Lane::of(role.lane(field.team), field.team, radiant, dire))
}
