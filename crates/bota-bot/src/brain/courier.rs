//! Sending the courier.
//!
//! What a courier is for is buying without walking home. Anything bought away
//! from the shop falls into a stash the hero cannot reach from the lane; the
//! courier is what turns that stash back into items in hand, and the whole of
//! the shopping list past the first trip depends on it.
//!
//! It is driven the same way anything else is: by casting what it carries at
//! it. The four errands are abilities on the courier, so an order for one
//! names the courier and the slot the errand sits in — and the slot is read
//! off the courier rather than assumed, since which slot holds which errand is
//! the server's business.

use bota_proto::{AbilitySlot, OrderTarget, SlotId, UnitKind, UnitView};

use crate::{BURST, DELIVER, GO_HOME, Params, Sight, TAKE_STASH, Want, span};

/// The courier this seat drives, while it is standing.
pub fn mine<'a>(sight: &Sight<'a>, slot: SlotId) -> Option<&'a UnitView> {
    sight
        .view
        .units
        .iter()
        .find(|unit| unit.kind == UnitKind::Courier && unit.owner == Some(slot) && unit.hp > 0)
}

/// Which slot of a courier holds one errand.
pub fn errand_slot(courier: &UnitView, errand: u16) -> Option<AbilitySlot> {
    courier
        .abilities
        .iter()
        .position(|ability| ability.id.0 == errand)
        .map(|at| AbilitySlot(at as u8))
}

/// Whether an errand may be given right now: the courier carries it and it is
/// not still on the clock.
fn ready(courier: &UnitView, errand: u16) -> Option<AbilitySlot> {
    let at = errand_slot(courier, errand)?;
    courier
        .abilities
        .get(usize::from(at.0))
        .filter(|ability| ability.cooldown_left == 0)
        .map(|_| at)
}

/// How many things are waiting in the stash.
pub fn waiting_in_stash(sight: &Sight) -> usize {
    sight
        .seat
        .stash
        .as_ref()
        .map_or(0, |slots| slots.iter().flatten().count())
}

/// How many things the courier is carrying.
pub fn carried(courier: &UnitView) -> usize {
    courier.items.iter().flatten().count()
}

/// What to tell the courier this tick, if anything.
///
/// One errand at a time and in one order: what it carries goes to its owner
/// first, because a courier holding the shopping is a courier that cannot
/// fetch the rest of it; only with empty hands is it worth sending to the
/// stash. A trip is not worth making for one thing, so it waits until enough
/// has piled up or until the first of it has waited long enough.
pub fn send_the_courier(
    sight: &Sight,
    slot: SlotId,
    since: Option<u32>,
    told: Option<(u16, u32)>,
    params: &Params,
) -> Option<Want> {
    let courier = mine(sight, slot)?;
    if carried(courier) > 0 {
        return deliver(sight, courier, told, params);
    }
    if still_at_it(told, TAKE_STASH, sight.view.tick, params) {
        return None;
    }
    let waiting = waiting_in_stash(sight);
    if waiting == 0 {
        return None;
    }
    let piled_up = waiting as f32 >= params.courier_batch;
    let waited = since.is_some_and(|since| {
        sight.view.tick.saturating_sub(since) as f32 >= params.courier_patience
    });
    if !piled_up && !waited {
        return None;
    }
    Some(Want::Errand {
        courier: courier.id,
        slot: ready(courier, TAKE_STASH)?,
    })
}

/// Bringing what it holds to the bot, and hurrying if the way is long.
///
/// It is held back while the bot is being shot at: a courier walks to where
/// its owner stands, and where its owner stands is where the shooting is.
fn deliver(
    sight: &Sight,
    courier: &UnitView,
    told: Option<(u16, u32)>,
    params: &Params,
) -> Option<Want> {
    if sight.under_fire(params) > params.courier_dread {
        return None;
    }
    let gap = span(courier.pos, sight.me.pos);
    if gap >= params.burst_gap
        && let Some(slot) = ready(courier, BURST)
    {
        return Some(Want::Errand {
            courier: courier.id,
            slot,
        });
    }
    if still_at_it(told, DELIVER, sight.view.tick, params) {
        return None;
    }
    Some(Want::Errand {
        courier: courier.id,
        slot: ready(courier, DELIVER)?,
    })
}

/// Whether an errand given a moment ago is still being carried out.
///
/// What a courier was told outlives the tick it was told in. Telling it again
/// changes nothing about what it does and costs the one order the seat has
/// that tick, which is an order the hero did not get to give.
fn still_at_it(told: Option<(u16, u32)>, errand: u16, now: u32, params: &Params) -> bool {
    told.is_some_and(|(given, when)| {
        given == errand && (now.saturating_sub(when) as f32) < params.courier_repeat
    })
}

/// Every errand the courier could be given this instant.
///
/// Legality only: whether a trip is worth making, and whether the way is safe,
/// are judgements, and a network choosing for itself makes its own.
pub fn ready_errands(sight: &Sight, slot: SlotId) -> Vec<Want> {
    let mut out = Vec::new();
    let Some(courier) = mine(sight, slot) else {
        return out;
    };
    let holding = carried(courier) > 0;
    let waiting = waiting_in_stash(sight) > 0;
    let mut errand = |which: u16| {
        if let Some(slot) = ready(courier, which) {
            out.push(Want::Errand {
                courier: courier.id,
                slot,
            });
        }
    };
    if holding {
        errand(DELIVER);
    }
    if waiting && !holding {
        errand(TAKE_STASH);
    }
    if holding || waiting {
        errand(BURST);
    }
    out
}

/// Sending it home, for when there is nothing else to do with it.
pub fn go_home(courier: &UnitView) -> Option<Want> {
    Some(Want::Errand {
        courier: courier.id,
        slot: ready(courier, GO_HOME)?,
    })
}

/// What an errand is aimed at: a courier's errands all work on the courier.
pub const AIMED_AT: OrderTarget = OrderTarget::None;
