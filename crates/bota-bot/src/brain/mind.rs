//! What the bot decides to do, from what it is allowed to see.
//!
//! Nothing here touches a socket. Everything it knows arrives as a
//! [`WorldView`], and everything it wants leaves as one [`Order`]: the server
//! keeps one order per seat per tick, so wanting two things at once means
//! wanting them in turn.
//!
//! The order of wants is fixed. A skill point and the shop come first because
//! they cost a tick and nothing else; then staying alive; then what is being
//! swung at already; then the creep about to fall; and only with none of that
//! to do does it walk to where it should be standing.

use bota_proto::{DamageKind, EntityId, EventKind, MatchInfo, SlotId, UnitView, Vec2, WorldView};

use crate::{Ask, Footing, Lane, Lanes, Params, Sight, Want};
use crate::{ROT, cast, deny, last_hit, mend, refill, shop, span, spend_a_point, swing_lead};
use crate::{SALVE, TANGO, out_of_tower_reach, slot_of, their_front, where_the_wave_is};
use crate::{send_the_courier, waiting_in_stash};

/// Ticks a second, until the server says otherwise.
const TICK_RATE: f32 = 30.0;

/// One bot's mind.
#[derive(Clone, Debug)]
pub struct Brain {
    /// Which seat it drives, once the server has told it.
    pub slot: Option<SlotId>,
    /// The numbers it plays by.
    pub params: Params,
    /// Ticks a second, as the match runs.
    tick_rate: f32,
    /// Every tree the map grew.
    forest: Vec<Vec2>,
    /// The ones still standing, and how many were down when that was worked
    /// out.
    standing: Vec<Vec2>,
    felled: usize,
    /// Which tick is being decided.
    tick: u32,
    /// What it asked for last, and the tick it asked on.
    said: Option<(Want, u32)>,
    /// The swing it has committed to: what it is aimed at, the tick it was
    /// begun on, and the tick to give up on.
    swinging: Option<Swing>,
    /// The body it is driving, so that it knows its own blows when it sees
    /// them.
    body: Option<EntityId>,
    /// The tick its own last blow landed on.
    struck_at: u32,
    /// The tick its next swing may begin on.
    ///
    /// The attack cycle is not on the wire. What is, is every blow that lands:
    /// one of its own says the cycle began a wind-up ago and comes round again
    /// an interval after that.
    ready_at: u32,
    /// Whether it is on its way out of the lane.
    leaving: bool,
    /// The tick the stash stopped being empty. Absent while it is.
    stash_since: Option<u32>,
    /// The errand the courier was last given, and the tick it was given on.
    told_the_courier: Option<(u16, u32)>,
    /// Which lane it is holding.
    lane: Option<usize>,
    /// Ticks between one swing beginning and the next.
    interval: u32,
    /// Whether its rot is burning.
    rot_burns: bool,
    /// What it remembers about getting where it meant to go.
    footing: Footing,
}

impl Default for Brain {
    fn default() -> Brain {
        Brain::new()
    }
}

impl Brain {
    /// A mind that knows nothing yet, playing by the numbers training left.
    pub fn new() -> Brain {
        Brain::with(Params::learned())
    }

    /// A mind that plays by a given set of numbers.
    pub fn with(params: Params) -> Brain {
        Brain {
            slot: None,
            params,
            tick_rate: TICK_RATE,
            forest: Vec::new(),
            standing: Vec::new(),
            felled: usize::MAX,
            tick: 0,
            said: None,
            swinging: None,
            body: None,
            struck_at: 0,
            ready_at: 0,
            leaving: false,
            stash_since: None,
            told_the_courier: None,
            lane: None,
            interval: 30,
            rot_burns: false,
            footing: Footing::new(),
        }
    }

    /// Takes in what the match is: the trees it may eat and the pace it runs
    /// at.
    pub fn match_started(&mut self, info: &MatchInfo) {
        self.forest = info.trees.clone();
        self.standing.clear();
        self.felled = usize::MAX;
        self.tick_rate = f32::from(info.tick_rate.max(1));
    }

    /// What to do this tick, if anything.
    pub fn decide(&mut self, view: &WorldView) -> Option<Ask> {
        self.tick = view.tick;
        self.remember_the_forest(view);
        let slot = self.slot?;
        let Some(sight) = Sight::new(view, slot, self.tick_rate) else {
            // Nothing standing: what a body remembers is not worth keeping for
            // the next one.
            self.footing.forget();
            self.said = None;
            self.swinging = None;
            self.body = None;
            self.stash_since = None;
            self.told_the_courier = None;
            self.struck_at = 0;
            self.ready_at = 0;
            self.leaving = false;
            self.rot_burns = false;
            return None;
        };
        self.body = Some(sight.me.id);
        self.interval = sight.me.attack_interval.max(1);
        let sight = Sight {
            wait: self.ready_at.saturating_sub(self.tick) as f32,
            ..sight
        };
        // Standing still with something in reach is a swing, not a body stuck
        // on the scenery: an order to walk stops where there is fighting to do.
        let swinging = sight.enemies().any(|foe| sight.in_reach(foe));
        let was_going_somewhere = !swinging
            && self
                .said
                .is_some_and(|(want, _)| want.is_a_walk() || matches!(want, Want::Hit(_)));
        self.footing
            .watch(sight.me, was_going_somewhere, &self.params);
        self.watch_the_stash(&sight);
        let want = self.want(&sight)?;
        let want = self.work_round(&sight, want);
        let ask = self.say(want)?;
        if let Want::Cast { slot, .. } = want
            && sight
                .me
                .abilities
                .get(usize::from(slot.0))
                .is_some_and(|ability| ability.id.0 == ROT)
        {
            self.rot_burns = !self.rot_burns;
        }
        if let Want::Errand { courier, slot } = want
            && let Some(errand) = sight
                .unit(courier)
                .and_then(|bird| bird.abilities.get(usize::from(slot.0)))
        {
            self.told_the_courier = Some((errand.id.0, self.tick));
        }
        Some(ask)
    }

    /// Keeps track of how long the shopping has been waiting at home.
    fn watch_the_stash(&mut self, sight: &Sight) {
        match waiting_in_stash(sight) {
            0 => self.stash_since = None,
            _ => self.stash_since = self.stash_since.or(Some(self.tick)),
        }
    }

    /// What happened during a tick that the snapshot does not show.
    ///
    /// Only its own blows are of any use here: one landing says where the
    /// attack cycle stands, which nothing else tells it.
    pub fn heard(&mut self, tick: u32, events: &[EventKind]) {
        let Some(me) = self.body else {
            return;
        };
        let interval = self.interval;
        for event in events {
            // A swing is what lands as physical damage; what an ability
            // leaves behind says nothing about the attack cycle.
            if let EventKind::Damaged {
                source,
                kind: DamageKind::Physical,
                ..
            } = event
                && *source == Some(me)
            {
                // The blow landed a wind-up after the swing began, and the
                // next one may begin an interval after that.
                self.struck_at = tick;
                self.ready_at = tick
                    .saturating_sub(self.params.swing_lead_ticks as u32)
                    .saturating_add(interval);
            }
        }
    }

    /// The one thing it wants most this tick.
    fn want(&mut self, sight: &Sight) -> Option<Want> {
        if let Some(want) = spend_a_point(sight.me) {
            return Some(want);
        }
        if let Some(want) = shop(sight) {
            return Some(want);
        }
        if let Some(want) = mend(sight, &self.standing, &self.params) {
            return Some(want);
        }
        if let Some(want) = refill(sight, &self.params) {
            return Some(want);
        }
        if let Some(want) = self.escape(sight) {
            return Some(want);
        }
        if let Some(want) = cast(sight, &self.params, self.rot_burns) {
            return Some(want);
        }
        if let Some(want) = self.finish_the_swing(sight) {
            return Some(want);
        }
        if let Some(creep) = deny(sight, &self.params) {
            return Some(self.swing_at(sight, creep));
        }
        if let Some(creep) = last_hit(sight, &self.params) {
            return Some(self.swing_at(sight, creep));
        }
        if let Some(slot) = self.slot
            && let Some(want) = send_the_courier(
                sight,
                slot,
                self.stash_since,
                self.told_the_courier,
                &self.params,
            )
        {
            return Some(want);
        }
        if let Some(want) = self.harass(sight) {
            return Some(want);
        }
        self.hold_the_lane(sight)
    }

    /// Commits to a swing, and keeps to it until it lands.
    fn swing_at(&mut self, sight: &Sight, on: &UnitView) -> Want {
        if self.swinging.is_none_or(|swing| swing.on != on.id) {
            let lead = swing_lead(sight, on, &self.params).ceil().max(1.0) as u32;
            self.swinging = Some(Swing {
                on: on.id,
                since: self.tick,
                give_up: self.tick + lead + self.interval,
            });
        }
        Want::Hit(on.id)
    }

    /// The swing already begun, while it is worth finishing.
    ///
    /// A swing given up halfway costs the whole of it: nothing was struck, the
    /// wait starts over, and the creep goes to whoever did not change its
    /// mind. So it is carried until the blow lands — which is what its own
    /// damage arriving means — or until the thing it was aimed at is gone.
    fn finish_the_swing(&mut self, sight: &Sight) -> Option<Want> {
        let swing = self.swinging?;
        let landed = self.struck_at >= swing.since;
        if landed || self.tick > swing.give_up || sight.unit(swing.on).is_none() {
            self.swinging = None;
            return None;
        }
        Some(Want::Hit(swing.on))
    }

    /// Leaving the lane while it is too worn down to hold it.
    ///
    /// It goes back until it is nearly whole again, and only all the way home
    /// when what it carries will not save it.
    fn escape(&mut self, sight: &Sight) -> Option<Want> {
        let hp = sight.hp_part();
        let coming = sight.under_fire(&self.params) * self.params.dread_seconds * sight.tick_rate;
        let cornered = hp < self.params.retreat_hp_part || (sight.me.hp as f32) < coming;
        if cornered {
            self.leaving = true;
        } else if hp >= self.params.return_hp_part {
            self.leaving = false;
        }
        if !self.leaving {
            return None;
        }
        // Falling back only pays while something will mend it out there. With
        // nothing to drink, the ground behind the lane heals no faster than
        // the ground in front of it, and the fountain heals properly.
        let carries_a_drink =
            slot_of(sight.me, TANGO).is_some() || slot_of(sight.me, SALVE).is_some();
        if (hp < self.params.go_home_part || !carries_a_drink)
            && let Some(home) = sight.fountain(sight.team)
        {
            return Some(Want::Walk(home));
        }
        let lanes = Lanes::seen(sight)?;
        let lane = self.my_lane(&lanes, sight).clone();
        let back = lane.how_far_along(sight.me.pos) - self.params.fall_back;
        Some(Want::Walk(lane.spot_at(back.max(0.0))))
    }

    /// Going at an enemy hero, while the odds are its own.
    ///
    /// It does not chase: what it will not walk out of the lane for it lets
    /// go.
    fn harass(&self, sight: &Sight) -> Option<Want> {
        let hero = sight
            .enemy_heroes()
            .filter(|hero| sight.gap_to(hero) <= sight.reach() + self.params.last_hit_slack)
            .filter(|hero| !under_their_tower(sight, hero.pos))
            .min_by_key(|hero| hero.hp)?;
        let edge = sight.hp_part() - crate::part(hero.hp, hero.max_hp);
        (edge >= self.params.harass_edge).then_some(Want::Hit(hero.id))
    }

    /// Standing where the lane is worth standing.
    ///
    /// Behind the front of the wave while there is one to stand behind, and
    /// far enough back that no tower of the other side reaches it. With
    /// nothing of theirs left on the lane it walks the wave forward instead.
    fn hold_the_lane(&mut self, sight: &Sight) -> Option<Want> {
        let lanes = Lanes::seen(sight)?;
        let lane = self.my_lane(&lanes, sight).clone();
        let wave = where_the_wave_is(sight, &lane, &self.params);
        let mut along = wave - self.params.stand_off;
        // Never inside the reach of what the other side has walked up: their
        // creeps come to the bot, and one standing among them is one being
        // swung at by all of them.
        if let Some(front) = their_front(sight, &lane, &self.params) {
            along = along.min(front - sight.reach() * self.params.keep_off_part);
        }
        let along = out_of_tower_reach(sight, &lane, along, &self.params);
        let at = lane.spot_at(along);
        let contested = their_front(sight, &lane, &self.params).is_some();
        if !contested {
            return Some(Want::Push(at));
        }
        if sight.how_far(at) <= self.params.arrive_radius {
            return Some(Want::Stop);
        }
        Some(Want::Walk(at))
    }

    /// The lane it is holding, chosen once and kept.
    fn my_lane<'a>(&mut self, lanes: &'a Lanes, sight: &Sight) -> &'a Lane {
        let picked = lanes.pick(sight, &self.params, self.lane);
        self.lane = Some(picked);
        lanes.at(picked)
    }

    /// The same want, with a way round whatever the body is caught on.
    fn work_round(&mut self, sight: &Sight, want: Want) -> Want {
        let towards = match want {
            Want::Walk(at) | Want::Push(at) => at,
            Want::Hit(on) => match sight.unit(on) {
                Some(body) => body.pos,
                None => return want,
            },
            _ => return want,
        };
        if let Some(at) = self.footing.detour() {
            return Want::Walk(at);
        }
        if !self.footing.is_wedged(&self.params) {
            return want;
        }
        Want::Walk(self.footing.work_loose(sight.me.pos, towards, &self.params))
    }

    /// Puts a want on the wire, unless it is the one already standing.
    ///
    /// The server keeps the last order until something replaces it, so saying
    /// the same thing again buys nothing and costs something: an order cancels
    /// the recovery of a swing and calls the creeps onto whoever gave it.
    fn say(&mut self, want: Want) -> Option<Ask> {
        if let Some((said, when)) = self.said
            && want.same_as(said, self.params.resend_drift)
            && self.tick.saturating_sub(when) < self.params.resend_ticks.max(1.0) as u32
        {
            return None;
        }
        self.said = Some((want, self.tick));
        Some(want.ask())
    }

    /// Keeps the list of standing trees in step with what has been felled.
    fn remember_the_forest(&mut self, view: &WorldView) {
        if self.felled == view.felled_trees.len() {
            return;
        }
        self.felled = view.felled_trees.len();
        self.standing = self
            .forest
            .iter()
            .enumerate()
            .filter(|(at, _)| !view.felled_trees.contains(&(*at as u32)))
            .map(|(_, tree)| *tree)
            .collect();
    }
}

/// A swing the bot has committed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Swing {
    /// What it is aimed at.
    on: EntityId,
    /// The tick it was begun on.
    since: u32,
    /// The tick to give it up on, when nothing has landed by then.
    give_up: u32,
}

/// Whether a spot is covered by a tower of the other side.
fn under_their_tower(sight: &Sight, at: Vec2) -> bool {
    sight
        .towers(sight.other_side())
        .any(|tower| span(at, tower.pos) <= tower.attack_range.to_f32())
}
