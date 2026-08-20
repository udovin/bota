//! Everything the bot could do this tick.
//!
//! The network chooses among candidates rather than inventing an order, so
//! this is where the space of orders is drawn. It is drawn to hold everything
//! the rule-driven bot can want and nothing that would be rejected: what is
//! legal stays the business of code that knows the rules, and what is wise
//! becomes the business of the network.
//!
//! The list is capped. A tick with forty creeps in sight is a tick where the
//! fortieth is not worth a row.

use bota_proto::{SlotId, Vec2};

use crate::{
    Lanes, Params, Sight, Want, aimed_cast, deniable, out_of_tower_reach, ready_errands, shop,
    spend_a_point, their_front, usable_items, where_the_wave_is,
};

/// The most candidates a tick is weighed up with.
pub const MOST_MOVES: usize = 24;
/// The most creeps of one side worth putting up as candidates.
const MOST_CREEPS: usize = 5;

/// Everything the bot could ask for this tick, standing first.
///
/// Standing is always in the list, so there is always something to choose and
/// the network is never asked to pick out of nothing.
pub fn moves(sight: &Sight, slot: SlotId, trees: &[Vec2], params: &Params) -> Vec<Want> {
    let mut out = vec![Want::Stop];
    if let Some(want) = spend_a_point(sight.me) {
        out.push(want);
    }
    if let Some(want) = shop(sight) {
        out.push(want);
    }
    out.extend(usable_items(sight, trees, params));
    out.extend(ready_errands(sight, slot));
    swings(sight, params, &mut out);
    spells(sight, params, &mut out);
    walks(sight, params, &mut out);
    out.truncate(MOST_MOVES);
    out
}

/// Swinging at what is worth swinging at: the enemy creeps nearest to falling,
/// its own that may be put out, and any enemy hero in reach.
fn swings(sight: &Sight, params: &Params, out: &mut Vec<Want>) {
    let reach = sight.reach() + params.last_hit_slack;
    let mut theirs: Vec<_> = sight
        .enemy_creeps()
        .filter(|creep| sight.gap_to(creep) <= reach)
        .collect();
    theirs.sort_by_key(|creep| creep.hp);
    out.extend(
        theirs
            .iter()
            .take(MOST_CREEPS)
            .map(|creep| Want::Hit(creep.id)),
    );
    let mut ours: Vec<_> = sight
        .own_creeps()
        .filter(|creep| sight.gap_to(creep) <= reach)
        .filter(|creep| deniable(creep, params))
        .collect();
    ours.sort_by_key(|creep| creep.hp);
    out.extend(ours.iter().take(3).map(|creep| Want::Hit(creep.id)));
    out.extend(
        sight
            .enemy_heroes()
            .filter(|hero| sight.gap_to(hero) <= reach)
            .map(|hero| Want::Hit(hero.id)),
    );
}

/// Casting what is ready, aimed the way that ability is aimed.
fn spells(sight: &Sight, params: &Params, out: &mut Vec<Want>) {
    for at in 0..sight.me.abilities.len() {
        if let Some(want) = aimed_cast(sight, at, params, None) {
            out.push(want);
        }
    }
}

/// Walking somewhere worth standing: up to the wave, back off it, and home.
///
/// Three spots rather than a map: the lane is a line, and where on it to stand
/// is the only part of walking that a match is decided by.
fn walks(sight: &Sight, params: &Params, out: &mut Vec<Want>) {
    let Some(lanes) = Lanes::seen(sight) else {
        return;
    };
    let lane = lanes.under(sight.me.pos);
    let wave = where_the_wave_is(sight, lane, params);
    let mut at = wave - params.stand_off;
    if let Some(front) = their_front(sight, lane, params) {
        at = at.min(front - sight.reach() * params.keep_off_part);
    }
    let held = out_of_tower_reach(sight, lane, at, params);
    let standing = lane.spot_at(held);
    out.push(Want::Walk(standing));
    out.push(Want::Push(standing));
    let back = (lane.how_far_along(sight.me.pos) - params.fall_back).max(0.0);
    out.push(Want::Walk(lane.spot_at(back)));
    if let Some(home) = sight.fountain(sight.team) {
        out.push(Want::Walk(home));
    }
    // Somewhere forward, for when the lane is theirs to lose.
    out.push(Want::Push(lane.spot_at(held + params.fall_back)));
}
