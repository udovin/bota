//! The numbers the bot plays by: written out, read back, and moved about.

use crate::{Dice, LEARNED, Params, Worth, score};

#[test]
fn a_written_set_reads_back_the_same() {
    let params = Params {
        stand_off: 333.0,
        retreat_hp_part: 0.42,
        ..Params::default()
    };
    let read = Params::parse(&params.to_text()).expect("what was written reads back");
    assert_eq!(read, params, "nothing is lost on the way through a file");
}

#[test]
fn a_set_may_leave_out_what_it_does_not_change() {
    let read = Params::parse("# only this one matters\nstand_off = 900\n")
        .expect("a name and a number is a set");
    assert_eq!(read.stand_off, 900.0, "what was given is taken");
    assert_eq!(
        read.retreat_hp_part,
        Params::default().retreat_hp_part,
        "and what was left out keeps the number it had"
    );
}

#[test]
fn a_set_naming_nothing_is_refused() {
    assert!(
        Params::parse("wisdom = 3\n").is_err(),
        "the bot does not play by numbers it has no use for"
    );
    assert!(
        Params::parse("stand_off = soon\n").is_err(),
        "and a number that is not one is not taken"
    );
}

#[test]
fn every_number_is_brought_inside_its_range() {
    let wild = Params::from_slice(&vec![f32::INFINITY; Params::count()]);
    for (value, (low, high)) in wild.clamped().to_vec().iter().zip(Params::RANGES) {
        assert!(
            value >= low && value <= high,
            "{value} is outside {low}..{high}"
        );
    }
}

#[test]
fn a_name_and_a_range_for_every_number() {
    assert_eq!(
        Params::NAMES.len(),
        Params::RANGES.len(),
        "every number is named and bounded"
    );
    assert_eq!(
        Params::default().to_vec().len(),
        Params::count(),
        "and every one of them comes out"
    );
}

#[test]
fn a_nudge_moves_a_few_numbers_and_leaves_the_rest() {
    let from = Params::default();
    let mut dice = Dice::from_seed(7);
    let moved = crate::nudge(&from, &mut dice, 2, 0.2);
    let differences = from
        .to_vec()
        .iter()
        .zip(moved.to_vec())
        .filter(|(was, now)| *was != now)
        .count();
    assert!(
        differences <= 2,
        "a challenger differs in at most what it was told to move"
    );
    assert!(differences > 0, "and it does differ");
}

#[test]
fn the_numbers_that_are_kept_read_back() {
    // The guard on the file that ships with the crate: a knob renamed or taken
    // away leaves it naming something nothing is called, and a bot that fell
    // back to the plain numbers would play worse for no visible reason.
    let read = Params::parse(LEARNED);
    assert!(
        read.is_ok(),
        "the numbers kept beside the crate do not read: {:?}",
        read.err()
    );
    assert_eq!(
        Params::learned(),
        read.unwrap_or_default(),
        "and what the bot plays by is what the file holds"
    );
}

#[test]
fn the_same_seed_walks_the_same_path() {
    let one: Vec<u64> = (0..8)
        .scan(Dice::from_seed(3), |d, _| Some(d.next_u64()))
        .collect();
    let other: Vec<u64> = (0..8)
        .scan(Dice::from_seed(3), |d, _| Some(d.next_u64()))
        .collect();
    assert_eq!(
        one, other,
        "a search from one seed is a search anybody can run again"
    );
}

#[test]
fn a_match_never_joined_is_not_a_match_played_badly() {
    let nothing = crate::Outcome::default();
    assert_eq!(
        score(&nothing, &Worth::default()),
        f32::MIN,
        "a seat that never stood is worth nothing at all"
    );
}
