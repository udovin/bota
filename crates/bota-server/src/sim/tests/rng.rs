//! Hidden randomness: reproducible, separated by purpose, exact in rate.

use crate::sim::*;
use bota_proto::EntityId;

const KEY: [u8; 32] = [7u8; 32];
const OTHER_KEY: [u8; 32] = [8u8; 32];

fn unit(idx: u32) -> EntityId {
    EntityId { idx, generation: 1 }
}

fn draws(stream: &mut Stream, n: usize) -> Vec<u32> {
    (0..n).map(|_| stream.next_u32()).collect()
}

#[test]
fn the_same_key_and_match_give_the_same_stream() {
    let a = MatchRng::new(&KEY, 42);
    let b = MatchRng::new(&KEY, 42);
    assert_eq!(
        draws(&mut a.global(Purpose::Rune), 8),
        draws(&mut b.global(Purpose::Rune), 8)
    );
}

#[test]
fn a_different_match_gives_a_different_stream() {
    let a = MatchRng::new(&KEY, 42);
    let b = MatchRng::new(&KEY, 43);
    assert_ne!(
        draws(&mut a.global(Purpose::Rune), 8),
        draws(&mut b.global(Purpose::Rune), 8)
    );
}

#[test]
fn a_different_key_gives_a_different_stream() {
    let a = MatchRng::new(&KEY, 42);
    let b = MatchRng::new(&OTHER_KEY, 42);
    assert_ne!(
        draws(&mut a.global(Purpose::Rune), 8),
        draws(&mut b.global(Purpose::Rune), 8)
    );
}

#[test]
fn purposes_do_not_share_a_sequence() {
    let rng = MatchRng::new(&KEY, 42);
    assert_ne!(
        draws(&mut rng.global(Purpose::Rune), 8),
        draws(&mut rng.global(Purpose::NeutralSpawn), 8)
    );
}

#[test]
fn units_do_not_share_a_sequence() {
    let rng = MatchRng::new(&KEY, 42);
    assert_ne!(
        draws(&mut rng.for_unit(Purpose::Crit, unit(1), 0), 8),
        draws(&mut rng.for_unit(Purpose::Crit, unit(2), 0), 8)
    );
}

#[test]
fn sources_on_one_unit_do_not_share_a_sequence() {
    let rng = MatchRng::new(&KEY, 42);
    assert_ne!(
        draws(&mut rng.for_unit(Purpose::Crit, unit(1), 0), 8),
        draws(&mut rng.for_unit(Purpose::Crit, unit(1), 1), 8)
    );
}

#[test]
fn a_global_stream_never_collides_with_a_unit_stream() {
    let rng = MatchRng::new(&KEY, 42);
    assert_ne!(
        draws(&mut rng.global(Purpose::Crit), 8),
        draws(&mut rng.for_unit(Purpose::Crit, unit(0), 0), 8)
    );
}

#[test]
fn drawing_from_one_stream_does_not_move_another() {
    let rng = MatchRng::new(&KEY, 42);
    let expected = draws(&mut rng.for_unit(Purpose::Crit, unit(5), 0), 4);

    // Consume heavily elsewhere, the way an extra unit or a new draw site would.
    let mut noisy = rng.global(Purpose::Rune);
    let _ = draws(&mut noisy, 1000);
    let mut other_unit = rng.for_unit(Purpose::Crit, unit(6), 0);
    let _ = draws(&mut other_unit, 1000);

    assert_eq!(
        draws(&mut rng.for_unit(Purpose::Crit, unit(5), 0), 4),
        expected
    );
}

#[test]
fn below_stays_in_range() {
    let rng = MatchRng::new(&KEY, 42);
    let mut stream = rng.global(Purpose::Rune);
    for n in [1u32, 2, 3, 7, 64, 1000] {
        for _ in 0..200 {
            assert!(stream.below(n) < n, "below({n}) escaped its range");
        }
    }
}

#[test]
fn below_covers_its_range() {
    let rng = MatchRng::new(&KEY, 42);
    let mut stream = rng.global(Purpose::Rune);
    let mut seen = [false; 6];
    for _ in 0..500 {
        seen[stream.below(6) as usize] = true;
    }
    assert!(
        seen.iter().all(|s| *s),
        "some value of below(6) never came up"
    );
}

#[test]
#[should_panic(expected = "below(0) has no answer")]
fn below_zero_panics() {
    let rng = MatchRng::new(&KEY, 42);
    rng.global(Purpose::Rune).below(0);
}

fn chance_for(unit_idx: u32, ratio: Ratio) -> Chance {
    let rng = MatchRng::new(&KEY, 42);
    Chance::new(rng.for_unit(Purpose::Crit, unit(unit_idx), 0), ratio)
}

/// Rolls until the next attempt opens a whole block.
///
/// The opening block starts at a hidden offset and is therefore short, so a test
/// about whole blocks has to step over it first.
fn align_to_block(chance: &mut Chance, ratio: Ratio) {
    for _ in 0..=ratio.den() {
        if chance.block_position() == 0 {
            return;
        }
        chance.roll(ratio);
    }
    panic!("never reached a block boundary");
}

#[test]
fn a_whole_block_holds_exactly_its_numerator() {
    let ratio = Ratio::new(3, 10);
    let mut chance = chance_for(1, ratio);

    align_to_block(&mut chance, ratio);

    for block in 0..50 {
        let hits = (0..ratio.den()).filter(|_| chance.roll(ratio)).count();
        assert_eq!(
            hits,
            ratio.num() as usize,
            "block {block} held {hits} hits instead of {}",
            ratio.num()
        );
    }
}

#[test]
fn the_rate_is_exact_for_many_ratios() {
    for (num, den) in [(1u8, 2u8), (3, 10), (1, 64), (63, 64), (7, 9), (2, 3)] {
        let ratio = Ratio::new(num, den);
        let mut chance = chance_for(num as u32 * 100 + den as u32, ratio);
        align_to_block(&mut chance, ratio);
        let blocks = 20;
        let hits = (0..blocks * den as usize)
            .filter(|_| chance.roll(ratio))
            .count();
        assert_eq!(hits, blocks * num as usize, "ratio {num}/{den}");
    }
}

#[test]
fn never_and_always_are_honoured() {
    let mut never = chance_for(1, Ratio::NEVER);
    let mut always = chance_for(2, Ratio::ALWAYS);
    for _ in 0..200 {
        assert!(!never.roll(Ratio::NEVER));
        assert!(always.roll(Ratio::ALWAYS));
    }
}

#[test]
fn a_full_block_of_sixty_four_is_honoured() {
    let all = Ratio::new(64, 64);
    let mut chance = chance_for(3, all);
    for _ in 0..200 {
        assert!(chance.roll(all));
    }
}

#[test]
fn the_order_within_a_block_differs_between_units() {
    let ratio = Ratio::new(3, 10);
    let mut a = chance_for(1, ratio);
    let mut b = chance_for(2, ratio);

    let pattern_a: Vec<bool> = (0..40).map(|_| a.roll(ratio)).collect();
    let pattern_b: Vec<bool> = (0..40).map(|_| b.roll(ratio)).collect();
    assert_ne!(
        pattern_a, pattern_b,
        "two units must not share a crit schedule"
    );
}

#[test]
fn the_order_within_a_block_differs_between_matches() {
    let ratio = Ratio::new(3, 10);
    let mut a = Chance::new(
        MatchRng::new(&KEY, 1).for_unit(Purpose::Crit, unit(1), 0),
        ratio,
    );
    let mut b = Chance::new(
        MatchRng::new(&KEY, 2).for_unit(Purpose::Crit, unit(1), 0),
        ratio,
    );

    let pattern_a: Vec<bool> = (0..40).map(|_| a.roll(ratio)).collect();
    let pattern_b: Vec<bool> = (0..40).map(|_| b.roll(ratio)).collect();
    assert_ne!(pattern_a, pattern_b);
}

#[test]
fn the_same_stream_replays_the_same_pattern() {
    let ratio = Ratio::new(3, 10);
    let mut a = chance_for(1, ratio);
    let mut b = chance_for(1, ratio);

    let pattern_a: Vec<bool> = (0..100).map(|_| a.roll(ratio)).collect();
    let pattern_b: Vec<bool> = (0..100).map(|_| b.roll(ratio)).collect();
    assert_eq!(pattern_a, pattern_b, "a replay must reproduce every crit");
}

#[test]
fn the_opening_block_starts_at_a_hidden_offset() {
    // Across units the first block is a different length, so an observer cannot
    // count attempts to find where blocks begin.
    let ratio = Ratio::new(1, 16);
    let firsts: Vec<usize> = (0..16)
        .map(|u| {
            let mut chance = chance_for(u, ratio);
            (0..ratio.den() as usize * 2)
                .position(|_| chance.roll(ratio))
                .unwrap_or(usize::MAX)
        })
        .collect();
    let distinct: std::collections::BTreeSet<_> = firsts.iter().collect();
    assert!(
        distinct.len() > 1,
        "every unit hit on the same attempt: {firsts:?}"
    );
}

#[test]
fn a_new_ratio_waits_for_the_block_boundary() {
    let slow = Ratio::new(1, 8);
    let fast = Ratio::new(8, 8);
    let mut chance = chance_for(1, slow);

    align_to_block(&mut chance, slow);
    for _ in 0..slow.den() {
        chance.roll(slow);
    }
    assert_eq!(
        chance.current(),
        slow,
        "the block in progress keeps its ratio"
    );

    // The first roll of the next block adopts the new ratio, and from there the
    // block is whole under it.
    assert!(chance.roll(fast));
    assert_eq!(chance.current(), fast);
    for _ in 0..fast.den() - 1 {
        assert!(chance.roll(fast));
    }
}

#[test]
#[should_panic(expected = "denominator above MAX_DEN")]
fn a_denominator_above_the_mask_width_panics() {
    let _ = Ratio::new(1, 65);
}

#[test]
#[should_panic(expected = "more hits than attempts")]
fn more_hits_than_attempts_panics() {
    let _ = Ratio::new(5, 4);
}

#[test]
#[should_panic(expected = "a ratio needs a denominator")]
fn a_zero_denominator_panics() {
    let _ = Ratio::new(0, 0);
}
