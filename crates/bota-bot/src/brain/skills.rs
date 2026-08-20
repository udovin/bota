//! Spending skill points, and spending mana.
//!
//! What an ability is aimed at and how far it reaches are not on the wire, so
//! the bot carries its own list of them, by [`AbilityId`]. An ability it has no
//! entry for is levelled like any other and never cast.

use bota_proto::{AbilityId, AbilitySlot, OrderTarget, UnitView, Vec2};

use crate::{Params, Sight, Want, along, span};

/// Sylla's critical strike, which is never cast.
pub const CRIT: u16 = 0;
/// Sylla's frenzy: swings come faster for a while.
pub const FRENZY: u16 = 1;
/// Sylla's bouncing bolt.
pub const BOUNCE: u16 = 2;
/// Sylla's volley, its ultimate.
pub const VOLLEY: u16 = 3;
/// Pudge's hook.
pub const MEAT_HOOK: u16 = 4;
/// Pudge's rot, a toggle that burns everything near, its owner included.
pub const ROT: u16 = 5;
/// Pudge's flesh heap, which is never cast.
pub const FLESH_HEAP: u16 = 6;
/// Pudge's dismember, its ultimate.
pub const DISMEMBER: u16 = 7;
/// A courier's turn of speed.
pub const BURST: u16 = 8;
/// A courier's walk home.
pub const GO_HOME: u16 = 9;
/// A courier taking what waits in the stash.
pub const TAKE_STASH: u16 = 10;
/// A courier handing over what it carries.
pub const DELIVER: u16 = 11;

/// Levels a plain ability may reach.
pub const ABILITY_MAX_LEVEL: u8 = 4;
/// Levels an ultimate may reach.
pub const ULT_MAX_LEVEL: u8 = 3;
/// The hero levels an ultimate waits for.
pub const ULT_FLOORS: [u8; 3] = [6, 8, 10];

/// Whether an ability is the ultimate, which waits on higher hero levels.
pub fn is_ultimate(id: AbilityId) -> bool {
    matches!(id.0, VOLLEY | DISMEMBER)
}

/// Whether an ability works on its own and is never cast.
pub fn is_passive(id: AbilityId) -> bool {
    matches!(id.0, CRIT | FLESH_HEAP)
}

/// Whether an ability is one a courier carries rather than a hero.
pub fn is_an_errand(id: AbilityId) -> bool {
    matches!(id.0, BURST | GO_HOME | TAKE_STASH | DELIVER)
}

/// The hero level one more level of an ability waits for.
pub fn level_floor(id: AbilityId, level: u8) -> u8 {
    if is_ultimate(id) {
        ULT_FLOORS
            .get(usize::from(level))
            .copied()
            .unwrap_or(u8::MAX)
    } else {
        2 * (level + 1) - 1
    }
}

/// Whether one more point may go into an ability now.
pub fn learnable(id: AbilityId, level: u8, hero_level: u8) -> bool {
    let cap = if is_ultimate(id) {
        ULT_MAX_LEVEL
    } else {
        ABILITY_MAX_LEVEL
    };
    level < cap && hero_level >= level_floor(id, level)
}

/// The slot the spare skill point goes into, if there is one to spend.
///
/// The ultimate takes every point it will: it waits on hero levels the others
/// do not, so a point held back for it is a point wasted.
pub fn spend_a_point(body: &UnitView) -> Option<Want> {
    let spent: u8 = body.abilities.iter().map(|ability| ability.level).sum();
    if spent >= body.level {
        return None;
    }
    let ready = |at: usize| {
        body.abilities
            .get(at)
            .is_some_and(|ability| learnable(ability.id, ability.level, body.level))
    };
    let ult = (0..body.abilities.len())
        .find(|at| body.abilities.get(*at).is_some_and(|a| is_ultimate(a.id)) && ready(*at));
    let next = ult.or_else(|| (0..body.abilities.len()).find(|at| ready(*at)))?;
    Some(Want::Level(AbilitySlot(next as u8)))
}

/// Whether an ability could be cast this instant: learned, off cooldown, and
/// paid for out of what the bot is willing to spend.
fn ready(sight: &Sight, at: usize, params: &Params) -> bool {
    let Some(ability) = sight.me.abilities.get(at) else {
        return false;
    };
    if ability.level == 0 || ability.cooldown_left > 0 || is_passive(ability.id) {
        return false;
    }
    let left = sight.me.mana - ability.mana_cost;
    if left < 0 {
        return false;
    }
    // Mana is kept back for the ultimate, and the ultimate spends it freely.
    let keeps_back = sight
        .me
        .abilities
        .iter()
        .any(|other| is_ultimate(other.id) && other.level > 0);
    if is_ultimate(ability.id) || !keeps_back {
        return true;
    }
    left as f32 >= params.mana_floor_part * sight.me.max_mana as f32
}

/// The spell worth casting this tick, if one is.
///
/// The list is walked in slot order and the first that answers wins, so what
/// the hero carries decides the order, not the policy.
pub fn cast(sight: &Sight, params: &Params, rot_burns: bool) -> Option<Want> {
    for at in 0..sight.me.abilities.len() {
        if !ready(sight, at, params) {
            continue;
        }
        let ability = sight.me.abilities[at];
        let slot = AbilitySlot(at as u8);
        let aimed = match ability.id.0 {
            FRENZY => at_myself(sight, params),
            BOUNCE | DISMEMBER => at_a_hero(sight, params),
            VOLLEY => at_a_crowd(sight, params),
            MEAT_HOOK => at_where_one_is_going(sight, params),
            ROT => rot_toggle(sight, params, rot_burns),
            _ => None,
        };
        if let Some(at) = aimed {
            return Some(Want::Cast { slot, at });
        }
    }
    None
}

/// Worth it while there is an enemy hero near enough to matter.
fn at_myself(sight: &Sight, params: &Params) -> Option<OrderTarget> {
    let near = sight
        .enemy_heroes()
        .any(|hero| sight.gap_to(hero) <= params.cast_reach);
    near.then_some(OrderTarget::None)
}

/// Aimed at the enemy hero nearest to falling that stands within reach.
fn at_a_hero(sight: &Sight, params: &Params) -> Option<OrderTarget> {
    sight
        .enemy_heroes()
        .filter(|hero| sight.gap_to(hero) <= params.cast_reach)
        .min_by_key(|hero| hero.hp)
        .map(|hero| OrderTarget::Unit { target: hero.id })
}

/// Worth it once enough of them stand inside it.
fn at_a_crowd(sight: &Sight, params: &Params) -> Option<OrderTarget> {
    let inside = sight
        .enemies()
        .filter(|unit| sight.gap_to(unit) <= params.cast_reach)
        .count();
    (inside as f32 >= params.volley_needs).then_some(OrderTarget::None)
}

/// Thrown where the nearest enemy hero will be, not where it stands.
fn at_where_one_is_going(sight: &Sight, params: &Params) -> Option<OrderTarget> {
    let hero = sight
        .enemy_heroes()
        .filter(|hero| span(sight.me.pos, hero.pos) <= params.hook_reach)
        .min_by_key(|hero| hero.hp)?;
    let ahead = lead_of(sight, hero, params);
    (span(sight.me.pos, ahead) <= params.hook_reach).then_some(OrderTarget::Point { pos: ahead })
}

/// Where a body will be after the lead a hook is thrown with.
///
/// Which way it is going is not on the wire; where it looks is, and a hero that
/// is walking looks the way it walks.
pub fn lead_of(sight: &Sight, body: &UnitView, params: &Params) -> Vec2 {
    let turns = f32::from(body.facing.brads) / 65536.0 * std::f32::consts::TAU;
    let reach = body.move_speed.to_f32() * params.hook_lead_ticks / sight.tick_rate.max(1.0);
    along(
        body.pos,
        crate::spot(
            body.pos.x.to_f32() + turns.cos() * 1000.0,
            body.pos.y.to_f32() + turns.sin() * 1000.0,
        ),
        reach,
    )
}

/// On while somebody is close enough to burn, off once nobody is.
///
/// The toggle is the same order either way, so the answer depends on what it
/// is doing now.
fn rot_toggle(sight: &Sight, params: &Params, burns: bool) -> Option<OrderTarget> {
    let caught = sight
        .enemies()
        .any(|unit| sight.gap_to(unit) <= params.rot_reach);
    let wants = caught && sight.hp_part() > params.retreat_hp_part;
    (wants != burns).then_some(OrderTarget::None)
}
