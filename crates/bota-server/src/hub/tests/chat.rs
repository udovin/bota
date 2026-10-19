//! Chat in the hall and in a match, and the commands a line may run.

use std::thread;
use std::time::Duration;

use bota_proto::{Auth, FrameReader, FromGame, FromHub, ReplayRecord, Role, SlotId, Team, ToHub};

use super::fixtures::*;
use crate::commands::COMMANDS;

/// A seat's gold as its own side sees it.
fn gold_of(view: &bota_proto::WorldView, slot: SlotId) -> i32 {
    view.players
        .iter()
        .find(|p| p.slot == slot)
        .and_then(|p| p.gold)
        .expect("a side sees its own gold")
}

/// A line as everybody hears it.
fn line(
    from: &str,
    side: Option<Team>,
    team: bool,
    text: &str,
) -> (String, Option<Team>, bool, String) {
    (from.to_string(), side, team, text.to_string())
}

#[test]
fn a_line_said_in_the_hall_reaches_the_hall_and_no_match() {
    let (addr, _dir) = hub("hall-chat");
    let (mut anna, _, _) = sign_up(addr, "anna");
    let (mut boris, _, _) = sign_up(addr, "boris");
    let (mut clara, _, _) = sign_up(addr, "clara");
    clara.send(&ToHub::CreateMatch {
        mode: REALTIME,
        name: "elsewhere".to_string(),
        seed: None,
        seats: 1,
    });
    clara.match_state();

    anna.chat("  hello all  ", true);
    let heard = line("anna", None, false, "hello all");
    assert_eq!(anna.said(), heard, "the speaker hears itself, trimmed");
    assert_eq!(boris.said(), heard, "and the hall has no sides");
    // Anna's line was handled before Clara speaks: had it reached her, it
    // would come ahead of her own.
    clara.chat("in here", false);
    assert_eq!(clara.said().3, "in here", "a match does not hear the hall");
}

#[test]
fn a_guest_reads_the_chat_but_does_not_write() {
    let (addr, _dir) = hub("guest-chat");
    let (mut anna, _, _) = sign_up(addr, "anna");
    let mut eye = TestClient::connect(addr);
    eye.send(&ToHub::Hello {
        auth: Auth::Guest {
            name: "anna".to_string(),
        },
    });
    eye.recv_until(|msg| matches!(msg, FromHub::Welcome { .. }).then_some(()));
    eye.chat("it is me, anna", false);
    assert_eq!(eye.notice(), "guests only watch");
    anna.chat("hi", false);
    assert_eq!(anna.said().3, "hi", "the guest's line reached nobody");
    assert_eq!(eye.said().3, "hi", "but the guest hears the hall");
}

#[test]
fn a_line_to_one_side_reaches_that_side_and_the_spectators() {
    let (addr, _dir) = hub("team-chat");
    let (mut host, mut guest, id) = gathered(addr, REALTIME);
    let (mut eye, _, _) = sign_up(addr, "eye");
    eye.send(&ToHub::JoinMatch {
        id,
        role: Role::Spectator,
    });
    eye.match_state();

    host.chat("push mid", true);
    let heard = line("alpha", Some(Team::Radiant), true, "push mid");
    assert_eq!(host.said(), heard);
    assert_eq!(eye.said(), heard, "a spectator hears either side");
    guest.chat("gl", false);
    assert_eq!(
        guest.said(),
        line("beta", Some(Team::Dire), false, "gl"),
        "the other side heard nothing before its own line"
    );
    assert_eq!(host.said().3, "gl");
    assert_eq!(eye.said().3, "gl");
}

#[test]
fn spectators_do_not_write_unless_the_mode_lets_them() {
    for (mode, writes) in [(REALTIME, false), (SANDBOX, true)] {
        let (addr, _dir) = hub("eye-writes");
        let (mut host, _guest, id) = gathered(addr, mode);
        let (mut eye, _, _) = sign_up(addr, "eye");
        eye.send(&ToHub::JoinMatch {
            id,
            role: Role::Spectator,
        });
        eye.match_state();
        eye.chat("go go", false);
        if writes {
            assert_eq!(host.said(), line("eye", None, false, "go go"));
        } else {
            assert_eq!(eye.notice(), "spectators do not write in this match");
            eye.chat("/roll", false);
            assert_eq!(
                eye.notice(),
                "spectators do not write in this match",
                "nor roll where they may not write"
            );
        }
    }
}

#[test]
fn a_command_answers_whoever_typed_it_and_a_roll_is_for_everybody() {
    let (addr, _dir) = hub("commands");
    let (mut anna, _, _) = sign_up(addr, "anna");
    let (mut boris, _, _) = sign_up(addr, "boris");

    anna.chat("/help", false);
    let listed: Vec<String> = (0..COMMANDS.len()).map(|_| anna.notice()).collect();
    assert!(listed[0].starts_with("/help"), "{listed:?}");
    assert!(listed.iter().any(|text| text.starts_with("/roll")));
    anna.chat("/nope", false);
    assert_eq!(anna.notice(), "no command /nope; /help lists them");
    anna.chat("/gold 5", false);
    assert_eq!(anna.notice(), "cheats work in a running match");

    anna.chat("/roll 6", false);
    let rolled = boris.notice();
    let drawn: u32 = rolled
        .strip_prefix("anna rolls ")
        .and_then(|rest| rest.strip_suffix(" (1-6)"))
        .and_then(|number| number.parse().ok())
        .unwrap_or_else(|| panic!("a roll told as {rolled:?}"));
    assert!((1..=6).contains(&drawn));
    assert_eq!(
        anna.notice(),
        rolled,
        "the one who rolled sees the same number"
    );
}

#[test]
fn a_match_that_plays_talks_on_its_game_server_side_by_side() {
    let (addr, _dir) = hub("game-chat");
    let (mut host, mut guest, id) = started(addr, SANDBOX);
    let (mut eye, _, _) = sign_up(addr, "eye");
    eye.send(&ToHub::JoinMatch {
        id,
        role: Role::Spectator,
    });
    eye.match_state();
    eye.match_start();

    host.chat("over here", false);
    assert_eq!(
        host.notice(),
        "a match that has started talks on its game server",
        "the hub does not carry it"
    );
    host.chat_game("push mid", true);
    let heard = line("alpha", Some(Team::Radiant), true, "push mid");
    assert_eq!(host.game_said(), heard);
    assert_eq!(eye.game_said(), heard, "a spectator hears either side");
    guest.chat_game("gl", false);
    assert_eq!(
        guest.game_said(),
        line("beta", Some(Team::Dire), false, "gl"),
        "the other side heard nothing before its own line"
    );
    assert_eq!(host.game_said().3, "gl");
}

#[test]
fn a_match_talks_on_its_game_server_while_its_heroes_are_picked() {
    let (addr, _dir) = hub("pick-chat");
    let (mut host, mut guest, _) = gathered(addr, REALTIME);
    host.send(&ToHub::StartMatch);
    host.picking();
    guest.picking();
    host.chat("gl", false);
    assert_eq!(
        host.notice(),
        "a match that has started talks on its game server"
    );
    host.chat_game("gl hf", false);
    assert_eq!(
        guest.game_said(),
        line("alpha", Some(Team::Radiant), false, "gl hf")
    );
}

#[test]
fn cheats_are_refused_where_they_are_off_and_told_to_everybody_where_they_are_on() {
    let (addr, _dir) = hub("cheats-off");
    let (mut host, _guest, _) = started(addr, REALTIME);
    host.chat_game("/gold 5000", false);
    assert_eq!(host.game_notice(), "cheats are off in this match");

    let (addr, _dir) = hub("cheats-on");
    let (mut host, mut guest, id) = started(addr, SANDBOX);
    let (mut eye, _, _) = sign_up(addr, "eye");
    eye.send(&ToHub::JoinMatch {
        id,
        role: Role::Spectator,
    });
    eye.match_state();
    eye.chat_game("/gold 5000 @0", false);
    assert_eq!(eye.game_notice(), "spectators do not cheat");
    host.chat_game("/gold 5000 @7", false);
    assert_eq!(host.game_notice(), "no seat answers to that");

    let before = gold_of(&host.snapshot(), SlotId(0));
    host.chat_game("/gold 5000", false);
    assert_eq!(host.game_notice(), "alpha: /gold 5000");
    assert_eq!(
        guest.game_notice(),
        "alpha: /gold 5000",
        "the other side is told"
    );
    assert_eq!(
        eye.game_notice(),
        "alpha: /gold 5000",
        "and so is a spectator"
    );
    let mut now = before;
    for _ in 0..30 {
        now = gold_of(&host.snapshot(), SlotId(0));
        if now >= before + 5000 {
            break;
        }
    }
    assert!(now >= before + 5000, "the gold lands within a tick or so");
}

#[test]
fn a_playing_match_keeps_its_chat_and_its_cheats_in_the_replay() {
    let (addr, dir) = hub("chat-replay");
    let (mut host, guest, id) = started(addr, SANDBOX);
    host.chat_game("gl hf", false);
    host.game_said();
    host.chat_game("/gold 100", false);
    host.game_notice();
    drop(host);
    drop(guest);
    let replay_path = dir.join("replays").join(format!("{}.brp", id.0));
    let mut waited = 0;
    while !replay_path.exists() {
        thread::sleep(Duration::from_millis(50));
        waited += 1;
        assert!(waited < 200, "the replay was never closed");
    }
    let mut reader = FrameReader::new();
    reader.push(&std::fs::read(&replay_path).expect("the file"));
    let mut said = false;
    let mut told = false;
    while let Ok(Some(record)) = reader.next_message::<ReplayRecord>() {
        match record {
            ReplayRecord::Msg(FromGame::Chat { from, text, .. }) => {
                said |= from == "alpha" && text == "gl hf";
            }
            ReplayRecord::Msg(FromGame::Notice { text }) => {
                told |= text == "alpha: /gold 100";
            }
            _ => {}
        }
    }
    assert!(said, "the line said is kept");
    assert!(told, "and the cheat typed");
}
