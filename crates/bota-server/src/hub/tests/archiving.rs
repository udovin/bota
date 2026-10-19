use std::thread;
use std::time::Duration;

use bota_proto::{Auth, FrameReader, FromHub, HallReason, MatchFilter, ReplayRecord, Stage, ToHub};

use super::fixtures::*;
use crate::archive::REPLAY_CHUNK;

#[test]
fn an_abandoned_match_can_be_fetched_whole_on_a_second_connection() {
    let (addr, dir) = hub("fetch");
    let (c1, c2, id) = started(addr, REALTIME);
    drop(c1);
    drop(c2);
    let replay_path = dir.join("replays").join(format!("{}.brp", id.0));
    let mut waited = 0;
    while !replay_path.exists() {
        thread::sleep(Duration::from_millis(50));
        waited += 1;
        assert!(waited < 200, "the replay was never closed");
    }
    // The hub learns of the end after the file is closed; a listing that
    // says so proves the archive has it.
    let (mut eye, _, token) = sign_up(addr, "eye");
    let mut listed = false;
    for _ in 0..100 {
        eye.send(&ToHub::ListMatches {
            filter: MatchFilter {
                stage: Some(Stage::Abandoned),
                tournament: None,
                account: None,
            },
            before: None,
            limit: 5,
        });
        let page = eye.recv_until(|msg| match msg {
            FromHub::Matches { page, .. } => Some(page),
            _ => None,
        });
        if page.iter().any(|s| s.id == id) {
            listed = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(listed);

    let mut fetcher = TestClient::connect(addr);
    fetcher.send(&ToHub::Hello {
        auth: Auth::Resume { token },
    });
    fetcher.recv_until(|msg| match msg {
        FromHub::Welcome { .. } => Some(()),
        _ => None,
    });
    fetcher.send(&ToHub::FetchReplay {
        match_id: bota_proto::MatchId(9999),
    });
    assert_eq!(fetcher.refusal(), HallReason::NoSuchReplay);
    fetcher.send(&ToHub::FetchReplay { match_id: id });
    let mut bytes = Vec::new();
    let total = loop {
        match fetcher.recv() {
            FromHub::ReplayChunk {
                match_id,
                offset,
                bytes: chunk,
            } => {
                assert_eq!(match_id, id);
                assert_eq!(offset, bytes.len() as u64);
                assert!(chunk.len() <= REPLAY_CHUNK);
                bytes.extend(chunk);
            }
            FromHub::ReplayEnd { match_id, total } => {
                assert_eq!(match_id, id);
                break total;
            }
            _ => {}
        }
    };
    assert_eq!(total, bytes.len() as u64);
    assert_eq!(bytes, std::fs::read(&replay_path).expect("the file"));
    let mut reader = FrameReader::new();
    reader.push(&bytes);
    assert!(matches!(
        reader.next_message::<ReplayRecord>(),
        Ok(Some(ReplayRecord::Msg(
            bota_proto::FromGame::MatchStart { .. }
        )))
    ));
}

#[test]
fn a_file_whose_summary_cannot_be_read_keeps_its_number_from_being_handed_out() {
    use crate::archive::Archive;
    use bota_proto::{MatchId, TournamentId};
    let dir = fresh_dir("archive-numbers");
    std::fs::create_dir_all(dir.join("replays")).expect("replays");
    std::fs::create_dir_all(dir.join("tournaments")).expect("tournaments");
    std::fs::write(dir.join("replays").join("9.meta"), b"not a summary").expect("meta");
    std::fs::write(dir.join("replays").join("12.brp"), b"old").expect("replay");
    std::fs::write(dir.join("replays").join("notes.txt"), b"").expect("notes");
    std::fs::write(dir.join("tournaments").join("4.meta"), b"not a cup").expect("cup");
    let mut archive = Archive::open(&dir).expect("open");
    assert_eq!(archive.next_match_id(), MatchId(13));
    assert_eq!(archive.next_tournament_id(), TournamentId(5));
}

#[test]
fn the_archive_pages_newest_first_and_marks_a_crashed_match_abandoned() {
    use crate::archive::Archive;
    use bota_proto::{MapId, MatchId, MatchSummary, ModeId};
    let dir = fresh_dir("archive");
    let summary = |id: u64, stage: Stage| MatchSummary {
        id: MatchId(id),
        name: format!("m{id}"),
        mode: ModeId(0),
        map: MapId(0),
        stage,
        host: None,
        seats: Vec::new(),
        tournament: None,
        created_at: 0,
        duration: 0,
        winner: None,
        seed: None,
    };
    {
        let mut archive = Archive::open(&dir).expect("open");
        assert_eq!(archive.next_match_id(), MatchId(1));
        for id in 1..=5 {
            archive.record(&summary(id, Stage::Over), None);
        }
        archive.record(&summary(6, Stage::Playing), None);
        std::fs::write(archive.part_path(MatchId(6)), b"partial").expect("part");
    }
    let mut archive = Archive::open(&dir).expect("reopen");
    assert_eq!(archive.next_match_id(), MatchId(7));
    let crashed = archive.summary(MatchId(6)).expect("kept");
    assert_eq!(crashed.stage, Stage::Abandoned);
    assert!(archive.has_replay(MatchId(6)), "the partial replay is kept");
    let (page, next) = archive.page(&MatchFilter::default(), None, 4);
    assert_eq!(
        page.iter().map(|s| s.id.0).collect::<Vec<_>>(),
        vec![6, 5, 4, 3]
    );
    assert_eq!(next, Some(MatchId(3)));
    let (rest, next) = archive.page(&MatchFilter::default(), next, 4);
    assert_eq!(rest.iter().map(|s| s.id.0).collect::<Vec<_>>(), vec![2, 1]);
    assert_eq!(next, None);
    let (over, _) = archive.page(
        &MatchFilter {
            stage: Some(Stage::Over),
            tournament: None,
            account: None,
        },
        None,
        10,
    );
    assert_eq!(over.len(), 5);
}
