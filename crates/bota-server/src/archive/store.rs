//! The data directory: its layout, the ids it hands out, and the index of
//! everything in it.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use bota_proto::{
    FrameReader, MatchFilter, MatchId, MatchStats, MatchSummary, Stage, TournamentId,
    TournamentInfo, encode_frame_to_vec,
};

/// The data directory and an index of the matches and tournaments in it.
///
/// Layout under the root: `replays/<id>.brp` is a finished replay and
/// `replays/<id>.brp.part` one still being written; `replays/<id>.meta` is
/// a [`MatchSummary`] frame followed by a [`MatchStats`] frame once the match
/// ended; `tournaments/<id>.meta` is a [`TournamentInfo`] frame;
/// `accounts.log` is the account ledger.
pub struct Archive {
    root: PathBuf,
    /// Every recorded match, sorted by id.
    index: Vec<MatchSummary>,
    /// Every stored tournament, sorted by id.
    tournaments: Vec<TournamentInfo>,
    next_match: u64,
    next_tournament: u64,
}

impl Archive {
    /// Opens the directory, creating it, and reads every summary in it.
    ///
    /// A match left on its game server by a crash is marked abandoned, and
    /// what its replay holds is kept under the finished name.
    pub fn open(root: &Path) -> io::Result<Archive> {
        fs::create_dir_all(root.join("replays"))?;
        fs::create_dir_all(root.join("tournaments"))?;
        let mut archive = Archive {
            root: root.to_path_buf(),
            index: Vec::new(),
            tournaments: Vec::new(),
            next_match: 1,
            next_tournament: 1,
        };
        for entry in fs::read_dir(root.join("replays"))? {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "meta") {
                continue;
            }
            let Some((mut summary, stats)) = read_meta(&path) else {
                continue;
            };
            if matches!(summary.stage, Stage::Picking | Stage::Playing) {
                summary.stage = Stage::Abandoned;
                archive.finish_replay(summary.id);
                archive.write_meta(&summary, stats.as_ref())?;
            }
            archive.index.push(summary);
        }
        archive.index.sort_by_key(|summary| summary.id);
        archive.index.dedup_by_key(|summary| summary.id);
        for entry in fs::read_dir(root.join("tournaments"))? {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "meta") {
                continue;
            }
            if let Some(info) = read_tournament(&path) {
                archive.tournaments.push(info);
            }
        }
        archive.tournaments.sort_by_key(|info| info.id);
        archive.next_match = archive
            .index
            .last()
            .map_or(0, |summary| summary.id.0)
            .max(highest_number(&root.join("replays"))?)
            + 1;
        archive.next_tournament = archive
            .tournaments
            .last()
            .map_or(0, |info| info.id.0)
            .max(highest_number(&root.join("tournaments"))?)
            + 1;
        Ok(archive)
    }

    /// Where the account ledger lives.
    pub fn accounts_path(&self) -> PathBuf {
        self.root.join("accounts.log")
    }

    /// A match id nobody has had.
    pub fn next_match_id(&mut self) -> MatchId {
        let id = MatchId(self.next_match);
        self.next_match += 1;
        id
    }

    /// A tournament id nobody has had.
    pub fn next_tournament_id(&mut self) -> TournamentId {
        let id = TournamentId(self.next_tournament);
        self.next_tournament += 1;
        id
    }

    /// The finished replay of a match.
    pub fn replay_path(&self, id: MatchId) -> PathBuf {
        self.root.join("replays").join(format!("{}.brp", id.0))
    }

    /// The replay of a match while it is being written.
    pub fn part_path(&self, id: MatchId) -> PathBuf {
        self.root.join("replays").join(format!("{}.brp.part", id.0))
    }

    /// Gives the replay of a match its finished name, or drops it when
    /// nothing of it was written.
    pub fn finish_replay(&self, id: MatchId) {
        let part = self.part_path(id);
        let written = fs::metadata(&part).is_ok_and(|meta| meta.len() > 0);
        if written {
            let _ = fs::rename(&part, self.replay_path(id));
        } else {
            let _ = fs::remove_file(&part);
        }
    }

    fn meta_path(&self, id: MatchId) -> PathBuf {
        self.root.join("replays").join(format!("{}.meta", id.0))
    }

    fn tournament_path(&self, id: TournamentId) -> PathBuf {
        self.root.join("tournaments").join(format!("{}.meta", id.0))
    }

    /// Records a match, replacing what was recorded of it before.
    pub fn record(&mut self, summary: &MatchSummary, stats: Option<&MatchStats>) {
        let _ = self.write_meta(summary, stats);
        match self.index.binary_search_by_key(&summary.id, |s| s.id) {
            Ok(at) => self.index[at] = summary.clone(),
            Err(at) => self.index.insert(at, summary.clone()),
        }
    }

    fn write_meta(&self, summary: &MatchSummary, stats: Option<&MatchStats>) -> io::Result<()> {
        let mut bytes = encode_frame_to_vec(summary).map_err(io::Error::other)?;
        if let Some(stats) = stats {
            bytes.extend(encode_frame_to_vec(stats).map_err(io::Error::other)?);
        }
        let path = self.meta_path(summary.id);
        let tmp = path.with_extension("meta.tmp");
        fs::File::create(&tmp)?.write_all(&bytes)?;
        fs::rename(&tmp, &path)
    }

    /// The recorded summary of a match.
    pub fn summary(&self, id: MatchId) -> Option<&MatchSummary> {
        self.index
            .binary_search_by_key(&id, |s| s.id)
            .ok()
            .map(|at| &self.index[at])
    }

    /// Whether a finished replay file exists for a match.
    pub fn has_replay(&self, id: MatchId) -> bool {
        self.replay_path(id).exists()
    }

    /// A page of the index, newest first: the matches and where the next
    /// page starts.
    pub fn page(
        &self,
        filter: &MatchFilter,
        before: Option<MatchId>,
        limit: u16,
    ) -> (Vec<MatchSummary>, Option<MatchId>) {
        let limit = usize::from(limit.clamp(1, 200));
        let mut page: Vec<MatchSummary> = self
            .index
            .iter()
            .rev()
            .filter(|s| before.is_none_or(|before| s.id < before))
            .filter(|s| matches_filter(s, filter))
            .take(limit + 1)
            .cloned()
            .collect();
        let next = if page.len() > limit {
            page.truncate(limit);
            page.last().map(|s| s.id)
        } else {
            None
        };
        (page, next)
    }

    /// Every tournament stored.
    pub fn tournaments(&self) -> &[TournamentInfo] {
        &self.tournaments
    }

    /// Forgets a tournament, on disk and in the index.
    pub fn forget_tournament(&mut self, id: TournamentId) {
        let _ = fs::remove_file(self.tournament_path(id));
        self.tournaments.retain(|info| info.id != id);
    }

    /// Stores a tournament, replacing what was stored of it before.
    pub fn save_tournament(&mut self, info: &TournamentInfo) {
        if let Ok(bytes) = encode_frame_to_vec(info) {
            let path = self.tournament_path(info.id);
            let tmp = path.with_extension("meta.tmp");
            let written = fs::File::create(&tmp)
                .and_then(|mut file| file.write_all(&bytes))
                .and_then(|()| fs::rename(&tmp, &path));
            let _ = written;
        }
        match self.tournaments.binary_search_by_key(&info.id, |t| t.id) {
            Ok(at) => self.tournaments[at] = info.clone(),
            Err(at) => self.tournaments.insert(at, info.clone()),
        }
    }
}

/// Whether a summary passes a filter.
pub fn matches_filter(summary: &MatchSummary, filter: &MatchFilter) -> bool {
    filter.stage.is_none_or(|stage| summary.stage == stage)
        && filter
            .tournament
            .is_none_or(|id| summary.tournament.is_some_and(|t| t.id == id))
        && filter.account.is_none_or(|_| true)
}

fn read_meta(path: &Path) -> Option<(MatchSummary, Option<MatchStats>)> {
    let mut bytes = Vec::new();
    fs::File::open(path).ok()?.read_to_end(&mut bytes).ok()?;
    let mut reader = FrameReader::new();
    reader.push(&bytes);
    let summary = reader.next_message::<MatchSummary>().ok()??;
    let stats = reader.next_message::<MatchStats>().ok().flatten();
    Some((summary, stats))
}

fn read_tournament(path: &Path) -> Option<TournamentInfo> {
    let mut bytes = Vec::new();
    fs::File::open(path).ok()?.read_to_end(&mut bytes).ok()?;
    let mut reader = FrameReader::new();
    reader.push(&bytes);
    reader.next_message::<TournamentInfo>().ok()?
}

/// The highest number a file of a directory is named with before its first
/// dot, readable or not. Zero when none is.
pub fn highest_number(dir: &Path) -> io::Result<u64> {
    let mut highest = 0;
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name();
        let number = name
            .to_str()
            .and_then(|name| name.split('.').next())
            .and_then(|stem| stem.parse::<u64>().ok());
        if let Some(number) = number {
            highest = highest.max(number);
        }
    }
    Ok(highest)
}
