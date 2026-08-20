//! What the bot buys, carries and drinks.
//!
//! Anything bought away from the shop falls into a stash the hero cannot reach
//! from the lane, so buying used to be worth doing only at home. With a
//! courier it is worth doing anywhere: the stash is where the courier collects
//! from, and the walk home is what the courier is for.
//!
//! What is owed is worked out from what it holds, stash and courier counted
//! in, rather than from how many times it has bought. Shopping that is already
//! on its way is shopping it has.

use bota_proto::{ItemId, ItemSlot, OrderTarget, UnitView, Vec2};

use crate::{Params, Sight, Want, span};

/// Boots of Speed.
pub const BOOTS: u16 = 0;
/// Clarity.
pub const CLARITY: u16 = 1;
/// Healing Salve.
pub const SALVE: u16 = 2;
/// Iron Branch.
pub const BRANCH: u16 = 3;
/// Quelling Blade.
pub const QUELLING: u16 = 5;
/// Tango.
pub const TANGO: u16 = 7;

/// The effect a salve or a tango leaves while it works.
pub const MENDING: u16 = 1;
/// The effect a clarity leaves while it works.
pub const CLARITY_EFFECT: u16 = 2;

/// Slots the hero carries on itself, the backpack included.
pub const BAG_SLOTS: usize = 9;
/// Slots of the inventory proper, where an item works.
pub const HELD_SLOTS: usize = 6;
/// How near the fountain buying is allowed, with room to spare.
pub const SHOP_REACH: f32 = 800.0;

/// What the bot wants to own, and how many of each.
pub const SHOPPING: [u16; 7] = [TANGO, BRANCH, BRANCH, SALVE, QUELLING, BOOTS, CLARITY];

/// What one item costs.
pub fn cost_of(item: u16) -> i32 {
    match item {
        BOOTS => 500,
        CLARITY => 50,
        SALVE => 110,
        BRANCH => 50,
        4 => 100,
        QUELLING => 225,
        6 => 50,
        TANGO => 90,
        8 => 100,
        _ => i32::MAX,
    }
}

/// Whether the bot stands where it may buy.
pub fn at_shop(sight: &Sight) -> bool {
    sight
        .fountain(sight.team)
        .is_some_and(|home| sight.how_far(home) <= SHOP_REACH)
}

/// How many of one item the bot holds, what is waiting and what is on its way
/// counted in.
fn holds(sight: &Sight, item: u16) -> usize {
    let counted = |slots: &[Option<bota_proto::ItemView>]| {
        slots
            .iter()
            .flatten()
            .filter(|held| held.id.0 == item)
            .count()
    };
    let bag = counted(&sight.me.items);
    let stash = sight.seat.stash.as_ref().map_or(0, |slots| counted(slots));
    let carried = sight
        .seat
        .unit
        .and_then(|_| crate::mine(sight, sight.seat.slot))
        .map_or(0, |courier| counted(&courier.items));
    bag + stash + carried
}

/// The first free slot the hero carries, or nothing while it carries none.
fn free_slot(body: &UnitView) -> Option<ItemSlot> {
    body.items
        .iter()
        .take(BAG_SLOTS)
        .position(|slot| slot.is_none())
        .map(|at| ItemSlot(at as u8))
}

/// The slot holding one item, of the ones where an item works.
pub fn slot_of(body: &UnitView, item: u16) -> Option<ItemSlot> {
    body.items
        .iter()
        .take(HELD_SLOTS)
        .position(|slot| slot.as_ref().is_some_and(|held| held.id.0 == item))
        .map(|at| ItemSlot(at as u8))
}

/// Whether one of the timed effects is on the bot right now.
pub fn under_effect(sight: &Sight, effect: u16) -> bool {
    sight.me.effects.iter().any(|on_it| on_it.id.0 == effect)
}

/// Taking what the stash holds, and buying what is still owed.
///
/// Standing at the shop, what waits in the stash is taken by hand: that is
/// quicker than sending for it, and the courier has better things to do.
/// Away from it, buying still pays — what is bought waits in the stash for the
/// courier, and gold that is spent cannot be dropped.
pub fn shop(sight: &Sight) -> Option<Want> {
    if at_shop(sight)
        && let Some(want) = take_from_stash(sight)
    {
        return Some(want);
    }
    let gold = sight.seat.gold.unwrap_or(0);
    let owed = SHOPPING
        .iter()
        .find(|item| holds(sight, **item) < owned_wanted(**item))?;
    if gold < cost_of(*owed) {
        return None;
    }
    // Somewhere for it to land: a slot on the hero when it is bought in hand,
    // a slot in the stash when it is bought from the lane — and, from the
    // lane, somebody to fetch it. Gold in hand is worth more than an item in a
    // stash nothing is coming for.
    let room = if at_shop(sight) {
        free_slot(sight.me).is_some()
    } else {
        free_stash_slot(sight).is_some() && crate::mine(sight, sight.seat.slot).is_some()
    };
    room.then_some(Want::Buy(ItemId(*owed)))
}

/// The first free slot of the stash, or nothing while it is full.
fn free_stash_slot(sight: &Sight) -> Option<usize> {
    sight
        .seat
        .stash
        .as_ref()?
        .iter()
        .position(|slot| slot.is_none())
}

/// How many of one item the shopping list asks for.
fn owned_wanted(item: u16) -> usize {
    SHOPPING.iter().filter(|listed| **listed == item).count()
}

/// Moving one thing out of the stash into a slot the hero carries.
fn take_from_stash(sight: &Sight) -> Option<Want> {
    let stash = sight.seat.stash.as_ref()?;
    let at = stash.iter().position(|slot| slot.is_some())?;
    let to = free_slot(sight.me)?;
    Some(Want::Fetch {
        from: ItemSlot((BAG_SLOTS + at) as u8),
        to,
    })
}

/// Drinking what it carries, when it is worn down enough to be worth one.
///
/// A tango eats a tree and nothing puts it out, so it is drunk wherever there
/// is a tree; a salve is broken by the first blow from anything that matters,
/// so it waits until nothing is shooting.
pub fn mend(sight: &Sight, trees: &[Vec2], params: &Params) -> Option<Want> {
    if sight.hp_part() >= params.mend_hp_part || under_effect(sight, MENDING) {
        return None;
    }
    if let (Some(slot), Some(tree)) = (slot_of(sight.me, TANGO), nearest_tree(sight, trees, params))
    {
        return Some(Want::Use {
            slot,
            at: OrderTarget::Point { pos: tree },
        });
    }
    if sight.under_fire(params) > 0.0 {
        return None;
    }
    slot_of(sight.me, SALVE).map(|slot| Want::Use {
        slot,
        at: OrderTarget::None,
    })
}

/// Drinking for mana, when there is room for it and nothing is shooting.
pub fn refill(sight: &Sight, params: &Params) -> Option<Want> {
    if sight.me.max_mana <= 0 || under_effect(sight, CLARITY_EFFECT) {
        return None;
    }
    if sight.mana_part() >= params.mana_floor_part || sight.under_fire(params) > 0.0 {
        return None;
    }
    slot_of(sight.me, CLARITY).map(|slot| Want::Use {
        slot,
        at: OrderTarget::None,
    })
}

/// The nearest tree still standing that a tango would reach.
pub fn nearest_tree(sight: &Sight, trees: &[Vec2], params: &Params) -> Option<Vec2> {
    trees
        .iter()
        .copied()
        .chain(sight.view.planted_trees.iter().copied())
        .filter(|at| span(sight.me.pos, *at) <= params.tree_reach)
        .min_by(|one, other| {
            span(sight.me.pos, *one)
                .partial_cmp(&span(sight.me.pos, *other))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}
