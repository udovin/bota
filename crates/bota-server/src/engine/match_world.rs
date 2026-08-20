//! The surface a match runs a world through.
//!
//! What the game loop asks of a world, answered by the engine. Only what has
//! been carried over answers for real; the rest is named here so the loop can
//! run, and says plainly that it does nothing yet.

use bota_proto::{MatchStats, Order, RejectReason, SlotId, SlotStats, Team};

use crate::engine::{Entity, PendingCast, Seat, UnitOrder, World};
use crate::sim::{Command, Event, MatchConfig, MatchRng, hero_spawn_pos, map_of, rules};

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
            let at = hero_spawn_pos(map, pick.team);
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
        // An order is an animation cancel: the recovery after a swing ends
        // with it. Giving up a swing that has not landed is the attack
        // cycle's own business.
        if let Some(state) = self.attacking.get_mut(unit) {
            state.recovering = 0;
        }
        // An order also takes a channelled teleport away, before whatever the
        // order itself does gets a chance to start another.
        self.teleport.remove(unit);
        let wanted = match cmd.order {
            Order::Move { pos } => UnitOrder::Move { pos },
            Order::AttackMove { pos } => UnitOrder::AttackMove { pos },
            Order::Stop => UnitOrder::Stand,
            Order::HoldPosition => UnitOrder::Hold,
            Order::AttackUnit { target } => {
                let Some(mark) = self.of_wire(target) else {
                    return;
                };
                self.rouse_bystanders(unit, mark);
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
            Order::UseItem { slot, target } => {
                self.use_item(unit, usize::from(slot.0), target);
                return;
            }
            Order::LevelUpAbility { slot } => {
                let mut events = Vec::new();
                self.learn(unit, usize::from(slot.0), &mut events);
                return;
            }
            Order::MoveItem { from, to } => {
                self.move_item(cmd.slot, usize::from(from.0), usize::from(to.0));
                return;
            }
            Order::SellItem { slot: at } => {
                self.sell_item(cmd.slot, usize::from(at.0));
                return;
            }
            Order::BuyItem { item } => {
                let mut events = Vec::new();
                self.buy(cmd.slot, item, &mut events);
                return;
            }
        };
        self.set_order(unit, wanted);
    }

    /// The seat at a slot, if that slot is in the match.
    pub fn seat(&self, slot: SlotId) -> Option<&Seat> {
        self.seats.iter().find(|s| s.slot == slot)
    }

    /// Whether a seat may issue an order right now.
    ///
    /// A seat with no body standing may order nothing, and a target it cannot
    /// see may as well not exist. Everything else is allowed to be asked for,
    /// whether or not it can be carried out.
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
                // Pointing an attack at one of your own is an order like any
                // other: what it cannot do is land, and that is settled when
                // targets are chosen. Turning it down here would take the
                // creeps' answer to it with it.
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
