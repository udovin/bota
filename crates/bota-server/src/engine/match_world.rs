//! The surface a match runs a world through.
//!
//! What the game loop asks of a world, answered by the engine. Only what has
//! been carried over answers for real; the rest is named here so the loop can
//! run, and says plainly that it does nothing yet.

use bota_proto::{MatchStats, Order, RejectReason, SlotId, SlotStats, Team, Vec2};

use crate::engine::{Entity, Orders, PendingCast, Seat, UnitOrder, World};
use crate::sim::{Command, Event, MatchConfig, MatchRng, fountain_pos, map_of, rules};

impl World {
    /// A world at tick zero for a match: the map standing, a seat per player,
    /// and a hero at each fountain.
    ///
    /// No camp is filled; the jungle has not been carried over yet.
    pub fn for_match(cfg: &MatchConfig, rng: MatchRng) -> World {
        let map = map_of(cfg.map);
        let mut world = World::on_map(map);
        world.rng = rng;
        for pick in &cfg.picks {
            let at = fountain_pos(map, pick.team);
            let hero = world.spawn_hero(pick.team, at, pick.slot, pick.hero);
            let mut seat = Seat::new(
                pick.slot,
                pick.team,
                pick.hero,
                rules::STARTING_GOLD,
                rules::STASH_SLOTS,
            );
            seat.unit = Some(hero);
            world.seats.push(seat);
        }
        world.settle();
        world
    }

    /// One tick of the match.
    ///
    /// Only orders that send a body somewhere are carried over; anything else
    /// is dropped.
    pub fn advance(&mut self, cmds: &[Command]) -> Vec<Event> {
        for cmd in cmds {
            self.take_order(cmd);
        }
        self.step()
    }

    /// Hands one order to the body of the seat that gave it.
    fn take_order(&mut self, cmd: &Command) {
        let Some(unit) = self
            .seats
            .iter()
            .find(|s| s.slot == cmd.slot)
            .and_then(|s| s.unit)
        else {
            return;
        };
        let wanted = match cmd.order {
            Order::Move { pos } => UnitOrder::Move { pos },
            Order::AttackMove { pos } => UnitOrder::AttackMove { pos },
            Order::Stop => UnitOrder::Idle,
            Order::HoldPosition => UnitOrder::Hold,
            Order::AttackUnit { target } => {
                let Some(mark) = self.of_wire(target) else {
                    return;
                };
                self.engage.insert(unit, mark);
                self.rouse_creeps(unit, mark);
                let at = self
                    .transform
                    .get(mark)
                    .map_or(bota_proto::Vec2::ZERO, |t| t.pos);
                UnitOrder::Attack {
                    target: mark,
                    last_seen: at,
                }
            }
            Order::CastAbility { slot, target } => {
                self.order_cast(unit, PendingCast { slot, target });
                return;
            }
            Order::UseItem { slot, .. } => {
                self.use_item(unit, usize::from(slot.0));
                return;
            }
            Order::LevelUpAbility { slot } => {
                let mut events = Vec::new();
                self.learn(unit, usize::from(slot.0), &mut events);
                return;
            }
            Order::BuyItem { item } => {
                let mut events = Vec::new();
                self.buy(cmd.slot, item, &mut events);
                return;
            }
            _ => return,
        };
        self.orders.insert(
            unit,
            Orders {
                current: wanted,
                cooldown: 0,
            },
        );
    }

    /// Wakes the lane creeps near an attack order onto whoever gave it.
    ///
    /// Ordering an attack at one of your own does not hand them the orderer:
    /// they rank everything else first.
    fn rouse_creeps(&mut self, orderer: Entity, mark: Entity) {
        let own = self.team.get(orderer).copied() == self.team.get(mark).copied();
        let at = self.transform.get(orderer).map_or(Vec2::ZERO, |t| t.pos);
        for creep in self.entities.iter().collect::<Vec<_>>() {
            if self.lane_ai.get(creep).is_none() || !self.hostile(creep, orderer) {
                continue;
            }
            // A creep answers an order it could have seen: its own acquisition
            // reaches the one who gave it.
            let reach = self
                .stats
                .get(creep)
                .map_or(bota_proto::Fixed::ZERO, |s| s.acquisition);
            if self
                .transform
                .get(creep)
                .is_some_and(|t| t.pos.within(at, reach))
            {
                self.provoke(creep, orderer, own);
            }
        }
    }

    /// The seat at a slot, if that slot is in the match.
    pub fn seat(&self, slot: SlotId) -> Option<&Seat> {
        self.seats.iter().find(|s| s.slot == slot)
    }

    /// Whether a seat may issue an order right now.
    ///
    /// A seat with no body standing may order nothing, and a target it cannot
    /// see may as well not exist.
    pub fn validate_order(&self, slot: SlotId, order: &Order) -> Result<(), RejectReason> {
        let Some(seat) = self.seats.iter().find(|s| s.slot == slot) else {
            return Err(RejectReason::HeroDead);
        };
        let Some(unit) = seat.unit else {
            return Err(RejectReason::HeroDead);
        };
        if !self.alive(unit) {
            return Err(RejectReason::HeroDead);
        }
        match order {
            Order::AttackUnit { target } => {
                let Some(entity) = self.of_wire(*target) else {
                    return Err(RejectReason::UnknownTarget);
                };
                if !self.can_see(seat.team, entity) {
                    return Err(RejectReason::UnknownTarget);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// The entity behind a handle from the wire, while it still stands.
    pub fn of_wire(&self, id: bota_proto::EntityId) -> Option<Entity> {
        self.entities
            .iter()
            .find(|entity| crate::engine::wire_id(*entity) == id)
    }

    /// The side that has won, if either has.
    pub fn victor(&self) -> Option<Team> {
        self.winner
    }

    /// Final numbers for every seat.
    pub fn match_stats(&self) -> MatchStats {
        MatchStats {
            duration: self.tick,
            slots: self
                .seats
                .iter()
                .map(|s| SlotStats {
                    slot: s.slot,
                    kills: s.kills,
                    deaths: s.deaths,
                    assists: s.assists,
                    last_hits: s.last_hits,
                    denies: s.denies,
                    net_worth: s.net_worth,
                    hero_damage: 0,
                    structure_damage: 0,
                })
                .collect(),
        }
    }
}
