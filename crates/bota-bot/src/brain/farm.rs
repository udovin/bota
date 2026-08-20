//! What a swing is worth, and when to take it.
//!
//! A last hit is a guess about the future: the swing lands some ticks after it
//! begins, and whatever else is already aimed at the creep lands in the
//! meantime. The bot guesses with the numbers it plays by, so a guess that is
//! too early or too late is something a search can correct.

use bota_proto::UnitView;

use crate::{Params, Sight, facing_at, facing_off, part, span};

/// Reach at or below which a swing lands where it stands rather than flying.
pub const MELEE_REACH: f32 = 350.0;

/// What is left of a blow after the armor it lands on.
pub fn after_armor(amount: f32, armor: f32, params: &Params) -> f32 {
    amount * 100.0 / (100.0 + params.armor_scale * armor.max(0.0))
}

/// What one of the bot's swings takes off a body.
pub fn my_blow_on(sight: &Sight, on: &UnitView, params: &Params) -> f32 {
    after_armor(sight.me.attack_damage as f32, on.armor.to_f32(), params)
}

/// Ticks between wanting a swing at a body and the blow landing on it.
///
/// The wait for the attack cycle to come round is part of it, and so is coming
/// round to face what is being swung at; an arrow spends more of them the
/// further it has to fly.
pub fn swing_lead(sight: &Sight, on: &UnitView, params: &Params) -> f32 {
    let off = facing_off(sight.me.facing, facing_at(sight.me.pos, on.pos));
    let mut ticks = sight.wait + params.swing_lead_ticks + params.turn_lead_ticks * off;
    if sight.reach() > MELEE_REACH && params.missile_speed > 0.0 {
        ticks += span(sight.me.pos, on.pos) / params.missile_speed * sight.tick_rate;
    }
    ticks
}

/// Damage a tick already aimed at a body by everything that reaches it.
pub fn incoming_on(sight: &Sight, on: &UnitView, params: &Params) -> f32 {
    sight
        .view
        .units
        .iter()
        .filter(|unit| unit.id != on.id && unit.team != on.team && unit.hp > 0)
        .filter(|unit| unit.attack_damage > 0 && unit.attack_interval > 0)
        .filter(|unit| {
            span(unit.pos, on.pos) - unit.radius.to_f32() - on.radius.to_f32()
                <= unit.attack_range.to_f32()
        })
        .map(|unit| {
            after_armor(unit.attack_damage as f32, on.armor.to_f32(), params)
                / unit.attack_interval as f32
        })
        .sum()
}

/// What a body will have left when a swing begun now lands on it.
pub fn hp_when_it_lands(sight: &Sight, on: &UnitView, params: &Params) -> f32 {
    let lead = swing_lead(sight, on, params);
    on.hp as f32 - incoming_on(sight, on, params) * lead * params.incoming_weight
}

/// Whether one swing at a body would be the one that takes it.
pub fn one_swing_takes_it(sight: &Sight, on: &UnitView, params: &Params) -> bool {
    let left = hp_when_it_lands(sight, on, params);
    left > 0.0 && left <= my_blow_on(sight, on, params) + params.last_hit_margin
}

/// How far the bot will walk out of its way for a swing.
fn worth_walking(sight: &Sight, on: &UnitView, params: &Params) -> bool {
    sight.gap_to(on) <= sight.reach() + params.last_hit_slack
}

/// The enemy creep the next swing would take, if one would.
///
/// Whatever is nearest to falling goes first: two creeps a swing from death
/// are one creep of gold missed.
pub fn last_hit<'a>(sight: &Sight<'a>, params: &Params) -> Option<&'a UnitView> {
    sight
        .enemy_creeps()
        .filter(|creep| worth_walking(sight, creep, params))
        .filter(|creep| one_swing_takes_it(sight, creep, params))
        .min_by(|one, other| order_by_hp(sight, one, other, params))
}

/// Whether one of its own is worn far enough down to be put out at all.
pub fn deniable(creep: &UnitView, params: &Params) -> bool {
    part(creep.hp, creep.max_hp) < params.deny_hp_part
}

/// One of its own that a swing would put out, if one would.
///
/// Only a creep worn far enough down may be denied at all; below that the
/// order is a walk towards it and nothing more.
pub fn deny<'a>(sight: &Sight<'a>, params: &Params) -> Option<&'a UnitView> {
    sight
        .own_creeps()
        .filter(|creep| deniable(creep, params))
        .filter(|creep| worth_walking(sight, creep, params))
        .filter(|creep| one_swing_takes_it(sight, creep, params))
        .min_by(|one, other| order_by_hp(sight, one, other, params))
}

/// Whichever of two bodies is nearer to falling, and the nearer one of two
/// equals.
fn order_by_hp(
    sight: &Sight,
    one: &UnitView,
    other: &UnitView,
    params: &Params,
) -> std::cmp::Ordering {
    let key = |unit: &UnitView| {
        (
            hp_when_it_lands(sight, unit, params),
            span(sight.me.pos, unit.pos),
        )
    };
    let (one_hp, one_far) = key(one);
    let (other_hp, other_far) = key(other);
    one_hp
        .partial_cmp(&other_hp)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then(
            one_far
                .partial_cmp(&other_far)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
        .then(one.id.idx.cmp(&other.id.idx))
}
