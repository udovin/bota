//! What the network is shown.
//!
//! Two halves. One describes the tick and is the same whatever the bot is
//! weighing up; the other describes one thing it could do. The network is fed
//! the two laid end to end, once per candidate, and answers with a number
//! saying how good that one looks. Nothing else about the world reaches it.
//!
//! Every number is brought to roughly the same size before it goes in. A
//! network fed gold in thousands and health in fractions spends its first
//! thousand steps learning the scale rather than the game.

use bota_proto::{OrderTarget, UnitKind, UnitView, Vec2};

use crate::{
    Lanes, Params, SALVE, Sight, TANGO, Want, carried, hp_when_it_lands, mine, my_blow_on, part,
    slot_of, span, waiting_in_stash, where_the_wave_is,
};

/// Numbers describing the tick.
pub const STATE_FEATURES: usize = 24;
/// Numbers describing one thing the bot could do.
///
/// The first [`KINDS`] of them say which kind it is; the rest describe it, and
/// which of them mean anything depends on the kind.
pub const MOVE_FEATURES: usize = 32;
/// What one row shown to the network is made of.
pub const FEATURES: usize = STATE_FEATURES + MOVE_FEATURES;

/// The kinds of thing the bot can want, as the network is told them.
///
/// One place in the row per kind, and the one that is this candidate's kind
/// holds one. Which kind a thing is, is the first thing worth knowing about
/// it, and a number saying "kind three" would have the network believe kind
/// three sits between kinds two and four.
pub const KINDS: usize = 10;

/// The furthest place in a row that describing a want ever writes to.
///
/// Named so that a test can hold the width to it: a want that writes past the
/// end takes the whole match down, and it takes down only the matches where
/// that kind of want came up.
pub const MOVE_KINDS_LAST: usize = KINDS + 20;

/// Which of those places a want takes.
pub fn kinds_of_want(want: &Want) -> usize {
    kind_of(want)
}

/// Which of those places a want takes.
fn kind_of(want: &Want) -> usize {
    match want {
        Want::Stop => 0,
        Want::Hold => 0,
        Want::Hit(_) => 1,
        Want::Walk(_) => 2,
        Want::Push(_) => 3,
        Want::Cast { .. } => 4,
        Want::Use { .. } => 5,
        Want::Buy(_) => 6,
        Want::Fetch { .. } => 7,
        Want::Level(_) => 8,
        Want::Errand { .. } => 9,
    }
}

/// What the tick looks like, whatever is being weighed up.
pub fn state_of(sight: &Sight, params: &Params) -> Vec<f32> {
    let mut out = vec![0.0; STATE_FEATURES];
    let me = sight.me;
    let home = sight.fountain(sight.team).unwrap_or(Vec2::ZERO);
    let lanes = Lanes::seen(sight);
    let lane = lanes.as_ref().map(|lanes| lanes.under(me.pos));
    let front = lane.map(|lane| where_the_wave_is(sight, lane, params));
    let along = lane.map(|lane| lane.how_far_along(me.pos));
    let hero = sight.enemy_heroes().min_by(|one, other| {
        sight
            .gap_to(one)
            .partial_cmp(&sight.gap_to(other))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let courier = sight.seat.unit.and_then(|_| mine(sight, sight.seat.slot));
    out[0] = sight.hp_part();
    out[1] = sight.mana_part();
    out[2] = f32::from(me.level) / 10.0;
    out[3] = sight.seat.gold.unwrap_or(0) as f32 / 1000.0;
    out[4] = sight.under_fire(params) / 5.0;
    out[5] = sight.view.tick as f32 / 18000.0;
    out[6] = sight
        .own_creeps()
        .filter(|c| sight.gap_to(c) < 1200.0)
        .count() as f32
        / 6.0;
    out[7] = sight
        .enemy_creeps()
        .filter(|c| sight.gap_to(c) < 1200.0)
        .count() as f32
        / 6.0;
    out[8] = f32::from(hero.is_some());
    out[9] = hero.map_or(1.0, |hero| sight.gap_to(hero) / 2000.0);
    out[10] = hero.map_or(0.0, |hero| part(hero.hp, hero.max_hp));
    out[11] = match (front, along) {
        (Some(front), Some(along)) => (along - front) / 2000.0,
        _ => 0.0,
    };
    out[12] = f32::from(under_their_tower(sight, me.pos));
    out[13] = span(me.pos, home) / 18000.0;
    out[14] = waiting_in_stash(sight) as f32 / 6.0;
    out[15] = f32::from(courier.is_some());
    out[16] = courier.map_or(0.0, |bird| f32::from(carried(bird) > 0));
    out[17] = courier.map_or(1.0, |bird| span(bird.pos, me.pos) / 18000.0);
    out[18] = me.items.iter().flatten().count() as f32 / 6.0;
    out[19] = f32::from(slot_of(me, TANGO).is_some());
    out[20] = f32::from(slot_of(me, SALVE).is_some());
    let spent: u8 = me.abilities.iter().map(|a| a.level).sum();
    out[21] = f32::from(spent < me.level);
    out[22] = sight.wait / 60.0;
    out[23] = f32::from(sight.wait <= 0.0);
    out
}

/// What one thing the bot could do looks like.
pub fn move_of(sight: &Sight, want: &Want, params: &Params) -> Vec<f32> {
    let mut out = vec![0.0; MOVE_FEATURES];
    out[kind_of(want)] = 1.0;
    match want {
        Want::Hit(target) => {
            if let Some(on) = sight.unit(*target) {
                let blow = my_blow_on(sight, on, params).max(1.0);
                out[KINDS] = sight.gap_to(on) / 1000.0;
                out[KINDS + 1] = part(on.hp, on.max_hp);
                out[KINDS + 2] = (hp_when_it_lands(sight, on, params) / blow).clamp(-4.0, 4.0);
                out[KINDS + 3] = f32::from(on.team == sight.team);
                out[KINDS + 4] = f32::from(on.kind == UnitKind::Hero);
                out[KINDS + 5] = f32::from(crate::is_wave_creep(on.kind));
                out[KINDS + 6] = f32::from(sight.in_reach(on));
                out[KINDS + 7] = incoming_share(sight, on, params);
            }
        }
        Want::Walk(pos) | Want::Push(pos) => {
            out[KINDS + 8] = sight.how_far(*pos) / 2000.0;
            out[KINDS + 9] = forward_by(sight, *pos, params) / 2000.0;
            out[KINDS + 10] = f32::from(under_their_tower(sight, *pos));
            out[KINDS + 11] = danger_at(sight, *pos) / 5.0;
        }
        Want::Cast { slot, at } => {
            if let Some(ability) = sight.me.abilities.get(usize::from(slot.0)) {
                out[KINDS + 12] = f32::from(slot.0) / 4.0;
                out[KINDS + 13] = f32::from(crate::is_ultimate(ability.id));
                out[KINDS + 14] = if sight.me.max_mana > 0 {
                    ability.mana_cost as f32 / sight.me.max_mana as f32
                } else {
                    0.0
                };
                out[KINDS + 15] = match at {
                    OrderTarget::Unit { target } => sight
                        .unit(*target)
                        .map_or(1.0, |on| sight.gap_to(on) / 1000.0),
                    OrderTarget::Point { pos } => sight.how_far(*pos) / 1000.0,
                    OrderTarget::None => 0.0,
                };
            }
        }
        Want::Errand { courier, slot } => {
            if let Some(bird) = sight.unit(*courier) {
                out[KINDS + 12] = f32::from(slot.0) / 4.0;
                out[KINDS + 16] = span(bird.pos, sight.me.pos) / 18000.0;
                out[KINDS + 17] = carried(bird) as f32 / 6.0;
                out[KINDS + 18] = waiting_in_stash(sight) as f32 / 6.0;
            }
        }
        Want::Use { slot, .. } => {
            out[KINDS + 12] = f32::from(slot.0) / 6.0;
            out[KINDS + 19] = 1.0 - sight.hp_part();
        }
        Want::Buy(item) => {
            out[KINDS + 12] = f32::from(item.0) / 8.0;
            out[KINDS + 20] = sight.seat.gold.unwrap_or(0) as f32 / 1000.0;
        }
        Want::Level(slot) => out[KINDS + 12] = f32::from(slot.0) / 4.0,
        Want::Fetch { from, .. } => out[KINDS + 12] = f32::from(from.0) / 15.0,
        Want::Stop | Want::Hold => {}
    }
    out
}

/// One row for the network: the tick and one thing to do about it.
pub fn row(state: &[f32], one: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(FEATURES);
    out.extend_from_slice(state);
    out.extend_from_slice(one);
    out
}

/// Whether a spot is covered by a tower of the other side.
fn under_their_tower(sight: &Sight, at: Vec2) -> bool {
    sight
        .towers(sight.other_side())
        .any(|tower| span(at, tower.pos) <= tower.attack_range.to_f32())
}

/// Damage a tick waiting at a spot, from everything that reaches it.
fn danger_at(sight: &Sight, at: Vec2) -> f32 {
    sight
        .foes()
        .filter(|unit| unit.attack_damage > 0 && unit.attack_interval > 0)
        .filter(|unit| {
            span(at, unit.pos) - sight.me.radius.to_f32() - unit.radius.to_f32()
                <= unit.attack_range.to_f32()
        })
        .map(|unit| unit.attack_damage as f32 / unit.attack_interval as f32)
        .sum()
}

/// How much further up the lane a spot is than where the bot stands.
fn forward_by(sight: &Sight, to: Vec2, params: &Params) -> f32 {
    let Some(lanes) = Lanes::seen(sight) else {
        return 0.0;
    };
    let _ = params;
    let lane = lanes.under(sight.me.pos);
    lane.how_far_along(to) - lane.how_far_along(sight.me.pos)
}

/// What part of the damage coming at a body is the bot's own.
fn incoming_share(sight: &Sight, on: &UnitView, params: &Params) -> f32 {
    let all = crate::incoming_on(sight, on, params);
    if all <= 0.0 {
        return 1.0;
    }
    (my_blow_on(sight, on, params) / (all * 30.0)).clamp(0.0, 4.0)
}
