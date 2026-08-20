//! The shape of the lanes, drawn from what both sides always see.
//!
//! Every building is on the wire for both sides, so the two fountains are
//! enough to lay out the three lanes: one straight between them and one round
//! each corner. A lane is a line to measure along — how far up it the wave has
//! got, how far back from the wave to stand, how far forward is somebody
//! else's tower.

use bota_proto::{UnitView, Vec2};

use crate::{Params, Sight, between, onto, span};

/// One lane as a line, from the bot's own fountain to the other side's.
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    /// The corners the lane runs through, its own end first.
    pub route: Vec<Vec2>,
}

impl Lane {
    /// How long the whole lane is.
    pub fn length(&self) -> f32 {
        self.route.windows(2).map(|leg| span(leg[0], leg[1])).sum()
    }

    /// How far along the lane a spot falls, measured from its own end.
    ///
    /// A spot off to one side counts as the nearest spot on the line.
    pub fn how_far_along(&self, at: Vec2) -> f32 {
        let mut walked = 0.0;
        let mut best = (f32::MAX, 0.0);
        for leg in self.route.windows(2) {
            let (part, off) = onto(leg[0], leg[1], at);
            let length = span(leg[0], leg[1]);
            if off < best.0 {
                best = (off, walked + length * part);
            }
            walked += length;
        }
        best.1
    }

    /// How far off the line a spot lies.
    pub fn off_the_line(&self, at: Vec2) -> f32 {
        self.route
            .windows(2)
            .map(|leg| onto(leg[0], leg[1], at).1)
            .fold(f32::MAX, f32::min)
    }

    /// The spot that far along the lane, its ends standing for anything past
    /// them.
    pub fn spot_at(&self, distance: f32) -> Vec2 {
        let first = self.route.first().copied().unwrap_or(Vec2::ZERO);
        let last = self.route.last().copied().unwrap_or(first);
        if distance <= 0.0 {
            return first;
        }
        let mut left = distance;
        for leg in self.route.windows(2) {
            let length = span(leg[0], leg[1]);
            if left <= length {
                let part = if length > 0.0 { left / length } else { 0.0 };
                return between(leg[0], leg[1], part);
            }
            left -= length;
        }
        last
    }
}

/// The three lanes, laid out from where the fountains stand.
#[derive(Clone, Debug, PartialEq)]
pub struct Lanes {
    /// One straight and one round each corner.
    pub lanes: Vec<Lane>,
}

impl Lanes {
    /// The lanes as this side walks them, or nothing while a fountain is
    /// missing from the snapshot.
    pub fn seen(sight: &Sight) -> Option<Lanes> {
        let home = sight.fountain(sight.team)?;
        let away = sight.fountain(sight.other_side())?;
        let corners = [
            Vec2 {
                x: home.x,
                y: away.y,
            },
            Vec2 {
                x: away.x,
                y: home.y,
            },
        ];
        Some(Lanes {
            lanes: vec![
                Lane {
                    route: vec![home, away],
                },
                Lane {
                    route: vec![home, corners[0], away],
                },
                Lane {
                    route: vec![home, corners[1], away],
                },
            ],
        })
    }

    /// The lane a spot stands on.
    pub fn under(&self, at: Vec2) -> &Lane {
        self.at(self.nearest_to(at))
    }

    /// Which lane the bot belongs on, given the one it is already on.
    ///
    /// A lane once chosen is kept. Walking from one to another costs half a
    /// minute and gives up whatever was standing on the first, so it takes
    /// more than a stray creep to be worth it: the lane in hand is left only
    /// once nothing of its own is on it and another plainly has a wave.
    pub fn pick(&self, sight: &Sight, params: &Params, held: Option<usize>) -> usize {
        let counted: Vec<usize> = self
            .lanes
            .iter()
            .map(|lane| {
                sight
                    .own_creeps()
                    .filter(|creep| lane.off_the_line(creep.pos) <= params.lane_width)
                    .count()
            })
            .collect();
        if let Some(held) = held.filter(|held| *held < self.lanes.len()) {
            let elsewhere = counted
                .iter()
                .enumerate()
                .filter(|(at, _)| *at != held)
                .map(|(_, count)| *count)
                .max()
                .unwrap_or(0);
            if counted[held] > 0 || elsewhere < 2 {
                return held;
            }
        }
        let most = counted.iter().copied().max().unwrap_or(0);
        if most > 0 {
            return counted.iter().position(|count| *count == most).unwrap_or(0);
        }
        self.nearest_to(sight.me.pos)
    }

    /// Which of the lanes a spot stands nearest to.
    pub fn nearest_to(&self, at: Vec2) -> usize {
        self.lanes
            .iter()
            .enumerate()
            .min_by(|(_, one), (_, other)| {
                one.off_the_line(at)
                    .partial_cmp(&other.off_the_line(at))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map_or(0, |(at, _)| at)
    }

    /// The lane at an index, or the straight one when there is no such lane.
    pub fn at(&self, index: usize) -> &Lane {
        self.lanes.get(index).unwrap_or(&self.lanes[0])
    }
}

/// How far up a lane the two sides meet, before either wave is in sight.
///
/// Halfway between the last tower of one side still standing and the first of
/// the other: that is where the waves will meet, and where the lane will keep
/// meeting once they do.
pub fn between_the_towers(sight: &Sight, lane: &Lane, params: &Params) -> f32 {
    let on_lane = |unit: &UnitView| lane.off_the_line(unit.pos) <= params.lane_width;
    let mine = sight
        .towers(sight.team)
        .filter(|tower| on_lane(tower))
        .map(|tower| lane.how_far_along(tower.pos))
        .fold(f32::MIN, f32::max);
    let theirs = sight
        .towers(sight.other_side())
        .filter(|tower| on_lane(tower))
        .map(|tower| lane.how_far_along(tower.pos))
        .fold(f32::MAX, f32::min);
    match (mine > f32::MIN, theirs < f32::MAX) {
        (true, true) => (mine + theirs) / 2.0,
        (true, false) => (mine + lane.length()) / 2.0,
        (false, true) => theirs / 2.0,
        (false, false) => lane.length() / 2.0,
    }
}

/// How far up a lane the fighting has reached.
///
/// Where the two waves touch, once both are in sight. Only one of them in
/// sight says less than it seems: the other is somewhere in the fog, and the
/// lane still meets where the towers say it meets, so the wave in sight only
/// moves the answer towards its own side.
pub fn where_the_wave_is(sight: &Sight, lane: &Lane, params: &Params) -> f32 {
    let on_lane = |unit: &UnitView| lane.off_the_line(unit.pos) <= params.lane_width;
    let mine = sight
        .own_creeps()
        .filter(|creep| on_lane(creep))
        .map(|creep| lane.how_far_along(creep.pos))
        .fold(f32::MIN, f32::max);
    let theirs = sight
        .enemy_creeps()
        .filter(|creep| on_lane(creep))
        .map(|creep| lane.how_far_along(creep.pos))
        .fold(f32::MAX, f32::min);
    let expected = between_the_towers(sight, lane, params);
    match (mine > f32::MIN, theirs < f32::MAX) {
        (true, true) => (mine + theirs) / 2.0,
        (true, false) => mine.max(expected),
        (false, true) => theirs.min(expected),
        (false, false) => expected,
    }
}

/// The same spot, pulled back down the lane until no enemy tower reaches it.
///
/// A spot that no tower reaches is left where it is. Nothing is pulled back
/// past the bot's own end of the lane.
pub fn out_of_tower_reach(sight: &Sight, lane: &Lane, along: f32, params: &Params) -> f32 {
    let step = 128.0;
    let mut along = along.max(0.0);
    for _ in 0..64 {
        let at = lane.spot_at(along);
        let covered = sight.towers(sight.other_side()).any(|tower| {
            span(at, tower.pos) <= tower.attack_range.to_f32() + params.tower_clearance
        });
        if !covered || along <= 0.0 {
            break;
        }
        along -= step;
    }
    along.max(0.0)
}

/// How far along the lane the nearest creep of the other side has come.
///
/// Absent while none of them is in sight.
pub fn their_front(sight: &Sight, lane: &Lane, params: &Params) -> Option<f32> {
    sight
        .enemy_creeps()
        .filter(|creep| lane.off_the_line(creep.pos) <= params.lane_width)
        .map(|creep| lane.how_far_along(creep.pos))
        .fold(None, |best: Option<f32>, at| {
            Some(best.map_or(at, |had| had.min(at)))
        })
}
