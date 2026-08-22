//! Take their towers, kill them, and keep itself whole.
//!
//! Four things at once, and towers are the point of it. A tower is worth more
//! the earlier it falls and more for every one already taken, so the second is
//! worth twice the first and the third three times it. Damage to a tower pays
//! on its own, and the hero is paid for walking at the nearest one, so a model
//! that has never taken a tower still has something telling it what to walk at.
//!
//! Two departures from the plain reading of "damage over time, times towers
//! taken". Multiplying the whole score by the number of towers taken makes
//! every point of damage before the first tower worth exactly nothing, which is
//! the flat plateau that stalled two earlier lessons; the multiplier applies to
//! the towers alone. And dividing by the clock makes a tower taken in the first
//! seconds worth unboundedly more than one taken a minute in, so a falloff that
//! halves is used instead.
//!
//! Health and mana are paid for only out of its own base. Paid wherever it
//! stood, the surest route to full health, full mana and no deaths at all is
//! never to leave the fountain.

use bota_proto::EventKind;

use super::common;
use crate::{Carried, MINUTE, Moment};

/// What one point of damage to one of their towers is worth.
const A_TOWER_HIT: f32 = 0.02;
/// What the first of their towers to fall is worth.
///
/// Multiplied by how many have fallen counting this one.
const A_TOWER_TAKEN: f32 = 50.0;
/// What killing one of them is worth.
const A_HERO_KILLED: f32 = 20.0;
/// What dying costs.
const A_DEATH: f32 = -10.0;
/// What one tick of full health and mana is worth, out where they can be lost.
const A_TICK_WHOLE: f32 = 0.002;
/// What closing one unit of the distance to their nearest tower is worth.
const A_STEP_HOME: f32 = 1e-3;
/// How far from its own fountain counts as out.
const OUT_OF_BASE: f32 = 2000.0;
/// The tick by which a tower is worth half of what it was worth at the horn.
const EARLY: f32 = 5.0 * MINUTE as f32;

/// What this tick was worth to this lesson.
pub fn score(now: &Moment, carried: &mut Carried) -> f32 {
    let soon = EARLY / (EARLY + now.tick() as f32);
    let mut marks = A_HERO_KILLED * f32::from(now.killed) + A_DEATH * f32::from(now.died);
    let me = now.field.me.map(|me| me.id);
    for event in now.events {
        match event {
            EventKind::Damaged {
                source,
                target,
                amount,
                ..
            } if *source == me
                && me.is_some()
                && common::is_a_tower_of_theirs(now.field, *target) =>
            {
                marks += A_TOWER_HIT * *amount as f32 * soon;
            }
            EventKind::Died { unit, denied, .. }
                if !*denied && common::is_a_tower_of_theirs(now.field, *unit) =>
            {
                carried.towers_down += 1;
                marks += A_TOWER_TAKEN * soon * f32::from(carried.towers_down);
            }
            _ => {}
        }
    }
    if !now.alive() {
        return marks;
    }
    let at = now.at();
    if now
        .field
        .home
        .is_some_and(|home| crate::span(at, home) > OUT_OF_BASE)
    {
        marks += A_TICK_WHOLE * common::wholeness(now.field);
    }
    if let Some(tower) = common::nearest_tower_of_theirs(now.field) {
        let off = crate::span(at, tower.pos);
        marks += A_STEP_HOME * common::closed(carried.was_off.replace(off), off);
    }
    marks
}
