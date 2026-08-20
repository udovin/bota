//! Everything the client knows, and how server messages change it.

use bota_proto::{
    ClientMsg, EntityId, EventKind, HeroId, LobbySlot, MatchStats, Order, PlayerId, ServerMsg,
    SlotId, Team, TickMode, UnitKind, WorldView,
};

use crate::camera::Camera;
use crate::net::Net;
use crate::replay_play::ReplayPlayer;

/// Where messages come from: a socket or a file.
pub enum Source {
    /// A live match.
    Live(Net),
    /// A recorded one.
    Replay(ReplayPlayer),
}

impl Source {
    /// Everything due since the last frame.
    pub fn poll(&mut self, dt: f32) -> Vec<ServerMsg> {
        match self {
            Source::Live(net) => net.poll(),
            Source::Replay(player) => player.poll(dt),
        }
    }

    /// Sends upstream. A replay swallows everything.
    pub fn send(&mut self, msg: &ClientMsg) {
        if let Source::Live(net) = self {
            net.send(msg);
        }
    }
}

/// Which screen the client is on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Waiting for seats, picks and readiness.
    Lobby,
    /// The match itself, including its end banner.
    Playing,
}

/// What the bottom panel is focused on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    /// Nothing picked: one's own seat.
    Own,
    /// A seat, picked from the top bar or by clicking its hero.
    Seat(SlotId),
    /// A unit that answers to no seat: a creep or a building.
    Unit(EntityId),
}

/// A damage number floating off a unit.
pub struct Floater {
    /// The text shown.
    pub text: String,
    /// World position it rises from.
    pub world: (f32, f32),
    /// Seconds since it appeared.
    pub age: f32,
}

/// One line of the kill feed.
pub struct FeedLine {
    /// The text shown.
    pub text: String,
    /// Seconds since it appeared.
    pub age: f32,
}

/// The whole client.
pub struct App {
    /// Socket or file.
    pub source: Source,
    /// Whether the server hung up on us.
    pub lost: bool,
    /// Current screen.
    pub phase: Phase,
    /// Our wire identity, once welcomed.
    pub me: Option<PlayerId>,
    /// Our seat, if playing.
    pub my_slot: Option<SlotId>,
    /// Ticks per second, from Welcome or MatchStart.
    pub tick_rate: u16,
    /// Ticks before the game clock reaches zero, from MatchStart.
    pub pregame_ticks: u32,
    /// A unit-target ability armed and waiting for a click.
    pub pending_ability: Option<u8>,
    /// The item slot waiting for a click to aim it, if one is.
    pub pending_item: Option<u8>,
    /// An item slot picked up and waiting for the destination click.
    pub held_item: Option<u8>,
    /// Whether the shop panel is open. Toggled by key or button; buying
    /// away from home lands in the stash.
    pub shop_open: bool,
    /// Every tree on the map, from MatchStart.
    pub trees: Vec<(f32, f32)>,
    /// Cells per terrain axis, from MatchStart.
    pub terrain_cells: usize,
    /// Decoded terrain cells, row-major from the south-west corner: bit 7
    /// walkable, bit 6 water, low bits the elevation tier.
    pub terrain: Vec<u8>,
    /// One bit per terrain cell: set where a tree or a fog blocker wall
    /// blocks sight lines.
    pub opaque: Vec<u64>,
    /// How the server advances ticks.
    pub mode: Option<TickMode>,
    /// The lobby as last broadcast.
    pub lobby: Vec<LobbySlot>,
    /// Seat names captured from the lobby for the scoreboard.
    pub names: Vec<(SlotId, String)>,
    /// The freshest state of the world.
    pub view: Option<WorldView>,
    /// The state one snapshot ago, for naming what died.
    pub prev_view: Option<WorldView>,
    /// The eye.
    pub camera: Camera,
    /// Sequence number of the next order.
    pub seq: u32,
    /// Whether the next left click is an attack-move.
    pub attack_move_armed: bool,
    /// What the bottom panel shows.
    pub selection: Selection,
    /// Whether readiness has been declared.
    pub ready: bool,
    /// Whether a hero pick has been sent.
    pub picked: bool,
    /// Damage numbers in flight.
    pub floaters: Vec<Floater>,
    /// Kill feed lines.
    pub feed: Vec<FeedLine>,
    /// The last rejected order, shown briefly: text and seconds left.
    pub reject: Option<(String, f32)>,
    /// The end of the match, once it came.
    pub over: Option<(Team, MatchStats)>,
    /// Set when the window should close.
    pub quit: bool,
}

impl App {
    /// A client over a source, before anything has been heard.
    pub fn new(source: Source) -> App {
        App {
            source,
            lost: false,
            phase: Phase::Lobby,
            me: None,
            my_slot: None,
            tick_rate: 30,
            pregame_ticks: 0,
            pending_ability: None,
            pending_item: None,
            held_item: None,
            shop_open: false,
            trees: Vec::new(),
            terrain_cells: 0,
            terrain: Vec::new(),
            opaque: Vec::new(),
            mode: None,
            lobby: Vec::new(),
            names: Vec::new(),
            view: None,
            prev_view: None,
            camera: Camera::over(9216.0, 9216.0),
            seq: 0,
            attack_move_armed: false,
            selection: Selection::Own,
            ready: false,
            picked: false,
            floaters: Vec::new(),
            feed: Vec::new(),
            reject: None,
            over: None,
            quit: false,
        }
    }

    /// The display name of a seat.
    pub fn seat_name(&self, slot: SlotId) -> String {
        self.names
            .iter()
            .find(|(s, _)| *s == slot)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| format!("seat {}", slot.0))
    }

    /// Our hero's unit id, while it is alive and known.
    pub fn my_hero(&self) -> Option<bota_proto::EntityId> {
        let slot = self.my_slot?;
        let view = self.view.as_ref()?;
        view.players.iter().find(|p| p.slot == slot)?.unit
    }

    /// Which side this player is on, once the match has told us.
    pub fn my_team(&self) -> Option<bota_proto::Team> {
        let slot = self.my_slot?;
        let view = self.view.as_ref()?;
        view.players.iter().find(|p| p.slot == slot).map(|p| p.team)
    }

    /// Whether the current selection is a unit this player commands.
    ///
    /// Orders go to the selection; anything not ours ignores them, so an
    /// enemy or a creep can be inspected without stealing the keys.
    pub fn controls_selection(&self) -> bool {
        match self.selection {
            Selection::Own => true,
            Selection::Seat(slot) => self.my_slot == Some(slot),
            Selection::Unit(_) => false,
        }
    }

    /// The team whose fog this client lives under. Absent for spectators,
    /// who see everything.
    pub fn fog_team(&self) -> Option<bota_proto::Team> {
        let slot = self.my_slot?;
        let view = self.view.as_ref()?;
        view.players.iter().find(|p| p.slot == slot).map(|p| p.team)
    }

    /// Whether our hero stands in its home shop area.
    pub fn at_home_shop(&self) -> bool {
        let Some(slot) = self.my_slot else {
            return false;
        };
        let Some(view) = &self.view else {
            return false;
        };
        let Some(p) = view.players.iter().find(|p| p.slot == slot) else {
            return false;
        };
        let Some(unit) = p.unit.and_then(|id| view.units.iter().find(|u| u.id == id)) else {
            return false;
        };
        let (fx, fy) = match p.team {
            bota_proto::Team::Radiant => (1760.0, 2278.0),
            bota_proto::Team::Dire => (16624.0, 16064.0),
            bota_proto::Team::Neutral => return false,
        };
        let dx = unit.pos.x.to_f32() - fx;
        let dy = unit.pos.y.to_f32() - fy;
        dx * dx + dy * dy <= 1000.0 * 1000.0
    }

    /// Whether the item in one of our slots is a consumable with charges.
    pub fn consumable_at(&self, slot: u8) -> bool {
        let Some(view) = &self.view else {
            return false;
        };
        let Some(my) = self.my_slot else {
            return false;
        };
        let Some(p) = view.players.iter().find(|p| p.slot == my) else {
            return false;
        };
        let item = if slot < 9 {
            p.unit
                .and_then(|id| view.units.iter().find(|u| u.id == id))
                .and_then(|u| u.items.get(usize::from(slot)).copied().flatten())
        } else {
            p.stash
                .as_ref()
                .and_then(|s| s.get(usize::from(slot - 9)).copied().flatten())
        };
        item.is_some_and(|i| i.charges > 0)
    }

    /// Which item sits in one of our slots, if one does.
    pub fn item_id_at(&self, slot: u8) -> Option<bota_proto::ItemId> {
        let view = self.view.as_ref()?;
        let my = self.my_slot?;
        let player = view.players.iter().find(|p| p.slot == my)?;
        let held = if slot < 9 {
            player
                .unit
                .and_then(|id| view.units.iter().find(|u| u.id == id))
                .and_then(|u| u.items.get(usize::from(slot)).copied().flatten())
        } else {
            player
                .stash
                .as_ref()
                .and_then(|s| s.get(usize::from(slot - 9)).copied().flatten())
        };
        held.map(|item| item.id)
    }

    /// How the item in one of our slots is aimed.
    pub fn aim_of(&self, slot: u8) -> crate::render::Aim {
        self.item_id_at(slot)
            .and_then(|id| crate::render::ITEM_AIM.get(usize::from(id.0)).copied())
            .unwrap_or(crate::render::Aim::Own)
    }

    /// Whether one of our fifteen item slots holds an item right now.
    pub fn item_at(&self, slot: u8) -> bool {
        let Some(view) = &self.view else {
            return false;
        };
        let Some(my) = self.my_slot else {
            return false;
        };
        let Some(p) = view.players.iter().find(|p| p.slot == my) else {
            return false;
        };
        if slot < 9 {
            let Some(unit) = p.unit.and_then(|id| view.units.iter().find(|u| u.id == id)) else {
                return false;
            };
            unit.items
                .get(usize::from(slot))
                .is_some_and(|s| s.is_some())
        } else {
            p.stash
                .as_ref()
                .and_then(|s| s.get(usize::from(slot - 9)))
                .is_some_and(|s| s.is_some())
        }
    }

    /// The seat the bottom panel falls back to: our own, else the first one.
    pub fn default_slot(&self) -> Option<SlotId> {
        self.my_slot
            .or_else(|| Some(self.view.as_ref()?.players.first()?.slot))
    }

    /// Sends an order with the next sequence number.
    pub fn send_order(&mut self, order: Order) {
        self.seq += 1;
        let seq = self.seq;
        self.source.send(&ClientMsg::Order { seq, order });
    }

    /// Applies one server message.
    pub fn handle(&mut self, msg: ServerMsg) {
        match msg {
            ServerMsg::Welcome {
                player_id,
                slot,
                tick_rate,
                mode,
            } => {
                self.me = Some(player_id);
                self.my_slot = slot;
                self.tick_rate = tick_rate;
                self.mode = Some(mode);
                if slot.is_some() && !self.picked {
                    self.picked = true;
                    self.source.send(&ClientMsg::PickHero { hero: HeroId(0) });
                }
            }
            ServerMsg::LobbyState { slots } => {
                self.names = slots
                    .iter()
                    .filter(|s| !s.name.is_empty())
                    .map(|s| (s.slot, s.name.clone()))
                    .collect();
                self.lobby = slots;
            }
            ServerMsg::MatchStart { info } => {
                self.tick_rate = info.tick_rate;
                self.pregame_ticks = info.pregame_ticks;
                self.trees = info
                    .trees
                    .iter()
                    .map(|t| (t.x.to_f32(), t.y.to_f32()))
                    .collect();
                self.terrain_cells = info.terrain_cells as usize;
                self.terrain = info
                    .terrain_rle
                    .iter()
                    .flat_map(|&(n, v)| std::iter::repeat_n(v, usize::from(n)))
                    .collect();
                let n = self.terrain_cells;
                self.opaque = vec![0u64; (n * n).div_ceil(64)];
                for &(cx, cy) in &info.opaque_cells {
                    let idx = usize::from(cy) * n + usize::from(cx);
                    self.opaque[idx / 64] |= 1 << (idx % 64);
                }
                self.phase = Phase::Playing;
            }
            ServerMsg::Snapshot { view } => {
                if self.mode == Some(TickMode::Lockstep) && self.my_slot.is_some() {
                    self.source.send(&ClientMsg::Ack { tick: view.tick });
                }
                self.prev_view = self.view.replace(view);
            }
            ServerMsg::Events { events, .. } => {
                for event in events {
                    self.absorb_event(event);
                }
            }
            ServerMsg::OrderRejected { reason, .. } => {
                self.reject = Some((format!("{reason:?}"), 2.5));
            }
            ServerMsg::MatchOver { winner, stats } => {
                self.over = Some((winner, stats));
            }
            ServerMsg::ParticipantLeft { slot, .. } => {
                if let Some(slot) = slot {
                    self.feed.push(FeedLine {
                        text: format!("{} disconnected", self.seat_name(slot)),
                        age: 0.0,
                    });
                }
            }
        }
    }

    fn absorb_event(&mut self, event: EventKind) {
        match event {
            EventKind::Damaged { target, amount, .. } => {
                if let Some(pos) = self.unit_pos(target) {
                    self.floaters.push(Floater {
                        text: format!("{amount}"),
                        world: pos,
                        age: 0.0,
                    });
                }
            }
            EventKind::Died { unit, killer, .. } => {
                let Some((kind, team, owner)) = self.unit_identity(unit) else {
                    return;
                };
                if kind != UnitKind::Hero {
                    return;
                }
                let victim = match owner {
                    Some(slot) => self.seat_name(slot),
                    None => format!("a {team:?} hero"),
                };
                let by =
                    killer
                        .and_then(|k| self.unit_identity(k))
                        .map(|(kind, team, owner)| match (kind, owner) {
                            (UnitKind::Hero, Some(slot)) => self.seat_name(slot),
                            (UnitKind::Tower, _) => format!("the {team:?} tower"),
                            (UnitKind::Fountain, _) => format!("the {team:?} fountain"),
                            (kind, _) => format!("a {team:?} {kind:?}"),
                        });
                let text = match by {
                    Some(by) => format!("{victim} was slain by {by}"),
                    None => format!("{victim} died"),
                };
                self.feed.push(FeedLine { text, age: 0.0 });
            }
            EventKind::StructureDestroyed { team, .. } => {
                self.feed.push(FeedLine {
                    text: format!("a {team:?} structure has fallen"),
                    age: 0.0,
                });
            }
            EventKind::LevelUp { unit, level } => {
                if let Some(pos) = self.unit_pos(unit) {
                    self.floaters.push(Floater {
                        text: format!("level {level}"),
                        world: pos,
                        age: 0.0,
                    });
                }
            }
            EventKind::Healed { .. }
            | EventKind::AbilityCast { .. }
            | EventKind::ItemBought { .. } => {}
        }
    }

    /// The position of a unit in the freshest view that still has it.
    fn unit_pos(&self, id: bota_proto::EntityId) -> Option<(f32, f32)> {
        for view in [self.view.as_ref(), self.prev_view.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(u) = view.units.iter().find(|u| u.id == id) {
                return Some((u.pos.x.to_f32(), u.pos.y.to_f32()));
            }
        }
        None
    }

    /// Kind, team and owner of a unit, from the freshest view that has it.
    fn unit_identity(&self, id: bota_proto::EntityId) -> Option<(UnitKind, Team, Option<SlotId>)> {
        for view in [self.view.as_ref(), self.prev_view.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(u) = view.units.iter().find(|u| u.id == id) {
                return Some((u.kind, u.team, u.owner));
            }
        }
        None
    }

    /// Notices a dead socket once and says so.
    pub fn check_connection(&mut self) {
        if self.lost || self.over.is_some() {
            return;
        }
        if let Source::Live(net) = &self.source
            && net.is_closed()
        {
            self.lost = true;
            self.feed.push(FeedLine {
                text: "connection to the server lost".to_string(),
                age: 0.0,
            });
        }
    }

    /// Ages and expires the transient effects.
    pub fn tick_effects(&mut self, dt: f32) {
        for f in &mut self.floaters {
            f.age += dt;
        }
        self.floaters.retain(|f| f.age < 1.2);
        for l in &mut self.feed {
            l.age += dt;
        }
        self.feed.retain(|l| l.age < 8.0);
        if let Some((_, left)) = &mut self.reject {
            *left -= dt;
            if *left <= 0.0 {
                self.reject = None;
            }
        }
    }
}
