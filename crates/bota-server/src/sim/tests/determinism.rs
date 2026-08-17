//! The same inputs give the same world, bit for bit.

use bota_proto::{Order, SlotId, Team, Vec2};

use super::fixtures::world;
use crate::sim::{Command, World, rules};

/// A scripted skirmish: both heroes push mid, re-ordered every few seconds.
fn scripted_commands(tick: u32) -> Vec<Command> {
    if !tick.is_multiple_of(100) {
        return Vec::new();
    }
    vec![
        Command {
            slot: SlotId(0),
            order: Order::AttackMove {
                pos: rules::DIRE_ANCIENT_POS,
            },
        },
        Command {
            slot: SlotId(1),
            order: Order::AttackMove {
                pos: Vec2::from_ints(4096, 4096),
            },
        },
    ]
}

fn run_scripted(ticks: u32) -> (World, Vec<u64>) {
    let mut w = world();
    let mut hashes = Vec::new();
    for t in 0..ticks {
        let cmds = scripted_commands(t);
        w.step(&cmds);
        if (t + 1).is_multiple_of(500) {
            hashes.push(w.hash());
        }
    }
    (w, hashes)
}

#[test]
fn two_runs_of_the_same_script_agree_at_every_checkpoint() {
    let (a, hashes_a) = run_scripted(2000);
    let (b, hashes_b) = run_scripted(2000);
    assert_eq!(hashes_a, hashes_b);
    assert_eq!(a.hash(), b.hash());
    assert_eq!(a.stats(), b.stats());
}

#[test]
fn checkpoints_differ_from_each_other() {
    let (_, hashes) = run_scripted(2000);
    for pair in hashes.windows(2) {
        assert_ne!(pair[0], pair[1], "the world keeps changing");
    }
}

#[test]
fn a_one_sided_push_wins_the_match() {
    let mut w = world();
    // Dire never acts; Radiant pushes with its creeps forever.
    let mut winner = None;
    for t in 0..60_000u32 {
        let cmds = if t.is_multiple_of(100) {
            vec![Command {
                slot: SlotId(0),
                order: Order::AttackMove {
                    pos: rules::DIRE_ANCIENT_POS,
                },
            }]
        } else {
            Vec::new()
        };
        w.step(&cmds);
        if let Some(team) = w.winner() {
            winner = Some((team, w.tick));
            break;
        }
    }
    let (team, tick) = winner.expect("an unopposed push must end the match");
    assert_eq!(team, Team::Radiant);
    assert!(tick > rules::FIRST_WAVE_TICK, "not before the first wave");
}
