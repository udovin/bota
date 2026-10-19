use std::cell::Cell;
use std::io::{self, Cursor, Read};
use std::rc::Rc;

use bota_proto::{
    FromGame, MapId, MatchInfo, ReplayRecord, TickMode, WorldView, encode_frame_to_vec,
};

pub fn match_start(tick_rate: u16) -> ReplayRecord {
    ReplayRecord::Msg(FromGame::MatchStart {
        info: MatchInfo {
            match_id: bota_proto::MatchId(1),
            map: MapId(0),
            tick_rate,
            pregame_ticks: 900,
            trees: Vec::new(),
            terrain_cells: 0,
            terrain_rle: Vec::new(),
            opaque_cells: Vec::new(),
            mode: TickMode::Lockstep,
            picks: Vec::new(),
            shop: Vec::new(),
            rules: bota_proto::Rules {
                waves: bota_proto::Lanes::ALL,
                invulnerable_towers: bota_proto::Lanes::NONE,
                death_limit: 0,
                tower_ends_it: false,
                time_limit: 0,
                cheats: false,
            },
        },
    })
}

pub fn snapshot(tick: u32) -> ReplayRecord {
    ReplayRecord::Msg(FromGame::Snapshot {
        view: WorldView {
            tick,
            viewer: None,
            units: Vec::new(),
            players: Vec::new(),
            projectiles: Vec::new(),
            loot: Vec::new(),
            felled_trees: Vec::new(),
            planted_trees: Vec::new(),
        },
    })
}

pub fn wire(records: &[ReplayRecord]) -> Vec<u8> {
    records
        .iter()
        .flat_map(|record| encode_frame_to_vec(record).unwrap())
        .collect()
}

pub struct CountedRead {
    pub source: Cursor<Vec<u8>>,
    pub bytes: Rc<Cell<usize>>,
    pub calls: Rc<Cell<usize>>,
    pub limit: usize,
    pub fragment: usize,
}

impl Read for CountedRead {
    fn read(&mut self, target: &mut [u8]) -> io::Result<usize> {
        self.calls.set(self.calls.get() + 1);
        let left = self.limit.saturating_sub(self.bytes.get());
        if left == 0 {
            return Err(io::Error::other("read crossed the permitted replay prefix"));
        }
        let count = target.len().min(left).min(self.fragment);
        let count = self.source.read(&mut target[..count])?;
        self.bytes.set(self.bytes.get() + count);
        Ok(count)
    }
}

pub fn counted(
    source: Vec<u8>,
    fragment: usize,
) -> (CountedRead, Rc<Cell<usize>>, Rc<Cell<usize>>) {
    let bytes = Rc::new(Cell::new(0));
    let calls = Rc::new(Cell::new(0));
    let reader = CountedRead {
        source: Cursor::new(source),
        bytes: bytes.clone(),
        calls: calls.clone(),
        limit: usize::MAX,
        fragment,
    };
    (reader, bytes, calls)
}

pub fn message(record: ReplayRecord) -> FromGame {
    match record {
        ReplayRecord::Msg(message) => message,
        ReplayRecord::Orders { tick, orders } => FromGame::Orders { tick, orders },
    }
}
