//! Generational storage: stable handles, reproducible order.

use crate::sim::Arena;
use bota_proto::EntityId;

#[test]
fn a_fresh_arena_is_empty() {
    let arena: Arena<u32> = Arena::new();
    assert_eq!(arena.len(), 0);
    assert!(arena.is_empty());
    assert!(arena.ids().is_empty());
}

#[test]
fn what_goes_in_comes_back_out() {
    let mut arena = Arena::new();
    let a = arena.insert("a");
    let b = arena.insert("b");

    assert_eq!(arena.len(), 2);
    assert_eq!(arena.get(a), Some(&"a"));
    assert_eq!(arena.get(b), Some(&"b"));
    assert!(arena.contains(a));
}

#[test]
fn a_handle_can_be_mutated_through() {
    let mut arena = Arena::new();
    let a = arena.insert(1u32);
    *arena.get_mut(a).unwrap() = 2;
    assert_eq!(arena.get(a), Some(&2));
}

#[test]
fn removal_takes_the_value_and_invalidates_the_handle() {
    let mut arena = Arena::new();
    let a = arena.insert("a");

    assert_eq!(arena.remove(a), Some("a"));
    assert_eq!(arena.len(), 0);
    assert_eq!(arena.get(a), None);
    assert!(!arena.contains(a));
    assert_eq!(arena.remove(a), None, "a second removal finds nothing");
}

#[test]
fn a_reused_slot_does_not_answer_to_the_old_handle() {
    let mut arena = Arena::new();
    let dead = arena.insert("creep");
    arena.remove(dead);
    let live = arena.insert("another creep");

    assert_eq!(live.idx, dead.idx, "the slot was reused");
    assert_ne!(live.generation, dead.generation, "but not the generation");
    assert_eq!(arena.get(dead), None, "a stale handle stays stale");
    assert_eq!(arena.get(live), Some(&"another creep"));
}

#[test]
fn a_stale_handle_cannot_remove_the_new_occupant() {
    let mut arena = Arena::new();
    let dead = arena.insert("creep");
    arena.remove(dead);
    let live = arena.insert("another creep");

    assert_eq!(arena.remove(dead), None);
    assert!(arena.contains(live), "the new occupant survived");
}

#[test]
fn a_zeroed_handle_is_never_valid() {
    let mut arena = Arena::new();
    arena.insert("a");
    let zeroed = EntityId {
        idx: 0,
        generation: 0,
    };
    assert_eq!(arena.get(zeroed), None);
}

#[test]
fn a_handle_past_the_end_is_not_valid() {
    let arena: Arena<u32> = Arena::new();
    let nowhere = EntityId {
        idx: 9999,
        generation: 1,
    };
    assert_eq!(arena.get(nowhere), None);
    assert!(!arena.contains(nowhere));
}

#[test]
fn iteration_runs_in_slot_order() {
    let mut arena = Arena::new();
    let ids: Vec<EntityId> = (0..8).map(|i| arena.insert(i)).collect();

    let walked: Vec<EntityId> = arena.iter().map(|(id, _)| id).collect();
    assert_eq!(walked, ids);

    let values: Vec<i32> = arena.iter().map(|(_, v)| *v).collect();
    assert_eq!(values, (0..8).collect::<Vec<_>>());
}

#[test]
fn iteration_skips_holes_and_stays_ordered() {
    let mut arena = Arena::new();
    let ids: Vec<EntityId> = (0..6).map(|i| arena.insert(i)).collect();
    arena.remove(ids[1]);
    arena.remove(ids[4]);

    let values: Vec<i32> = arena.iter().map(|(_, v)| *v).collect();
    assert_eq!(values, vec![0, 2, 3, 5]);
    assert_eq!(arena.len(), 4);
}

#[test]
fn iteration_order_does_not_depend_on_insertion_history() {
    // Two arenas holding the same entities in the same slots must walk the same
    // way, whatever sequence of inserts and removes got them there.
    let mut direct = Arena::new();
    for i in 0..4 {
        direct.insert(i);
    }

    let mut churned = Arena::new();
    let scratch: Vec<EntityId> = (0..4).map(|i| churned.insert(i * 100)).collect();
    for id in &scratch {
        churned.remove(*id);
    }
    for i in (0..4).rev() {
        churned.insert(i);
    }

    let a: Vec<i32> = direct.iter().map(|(_, v)| *v).collect();
    let b: Vec<i32> = churned.iter().map(|(_, v)| *v).collect();
    assert_eq!(a.len(), b.len());
    assert_eq!(
        churned.iter().map(|(id, _)| id.idx).collect::<Vec<_>>(),
        vec![0, 1, 2, 3],
        "slots are walked low to high regardless of history"
    );
}

#[test]
fn iter_mut_reaches_every_entity() {
    let mut arena = Arena::new();
    for i in 0..5 {
        arena.insert(i);
    }
    for (_, v) in arena.iter_mut() {
        *v *= 10;
    }
    let values: Vec<i32> = arena.iter().map(|(_, v)| *v).collect();
    assert_eq!(values, vec![0, 10, 20, 30, 40]);
}

#[test]
fn ids_snapshots_the_handles_for_a_mutating_pass() {
    let mut arena = Arena::new();
    let ids: Vec<EntityId> = (0..5).map(|i| arena.insert(i)).collect();

    // A pass that kills half of what it walks: the point of taking ids first.
    for id in arena.ids() {
        if arena.get(id).is_some_and(|v| v % 2 == 0) {
            arena.remove(id);
        }
    }

    assert_eq!(arena.len(), 2);
    assert_eq!(arena.get(ids[1]), Some(&1));
    assert_eq!(arena.get(ids[3]), Some(&3));
    assert_eq!(arena.get(ids[0]), None);
}

#[test]
fn slots_are_reused_before_the_arena_grows() {
    let mut arena = Arena::new();
    let ids: Vec<EntityId> = (0..4).map(|i| arena.insert(i)).collect();
    arena.remove(ids[2]);

    let fresh = arena.insert(42);
    assert_eq!(
        fresh.idx, 2,
        "the freed slot was taken rather than a new one"
    );
    assert_eq!(arena.len(), 4);
}
