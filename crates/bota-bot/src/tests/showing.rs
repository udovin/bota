//! What the network is shown: the right width, and nothing off the end of it.

use bota_proto::{AbilitySlot, EntityId, ItemId, ItemSlot, OrderTarget, Vec2};

use crate::{FEATURES, MOVE_FEATURES, MOVE_KINDS_LAST, Want, kinds_of_want};

/// One of every kind of want there is.
fn one_of_each() -> Vec<Want> {
    vec![
        Want::Stop,
        Want::Hold,
        Want::Hit(EntityId {
            idx: 1,
            generation: 1,
        }),
        Want::Walk(Vec2::from_ints(100, 100)),
        Want::Push(Vec2::from_ints(100, 100)),
        Want::Cast {
            slot: AbilitySlot(1),
            at: OrderTarget::None,
        },
        Want::Use {
            slot: ItemSlot(0),
            at: OrderTarget::None,
        },
        Want::Buy(ItemId(0)),
        Want::Fetch {
            from: ItemSlot(9),
            to: ItemSlot(0),
        },
        Want::Level(AbilitySlot(0)),
        Want::Errand {
            courier: EntityId {
                idx: 2,
                generation: 1,
            },
            slot: AbilitySlot(0),
        },
    ]
}

#[test]
fn every_kind_of_want_has_a_place_of_its_own() {
    let mut seen = Vec::new();
    for want in one_of_each() {
        let at = kinds_of_want(&want);
        assert!(at < MOVE_FEATURES, "{want:?} is written past the end");
        seen.push(at);
    }
    // Standing and holding share a place; nothing else does.
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen.len(),
        one_of_each().len() - 1,
        "kinds run into one another"
    );
}

#[test]
fn nothing_is_written_off_the_end_of_a_row() {
    // The furthest number any kind writes has to fit, or a match panics the
    // first time that kind comes up, which is what happened.
    let furthest = MOVE_KINDS_LAST;
    let width = MOVE_FEATURES;
    assert!(furthest < width, "a want writes to {furthest} of {width}");
    assert_eq!(
        FEATURES,
        crate::STATE_FEATURES + MOVE_FEATURES,
        "a row is the tick and one thing to do about it"
    );
}
