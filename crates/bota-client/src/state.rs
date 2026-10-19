//! Everything the client knows, and how server messages change it.

use std::path::PathBuf;

use bota_proto::{
    AccountInfo, EntityId, EventKind, FromGame, FromHub, GameReason, HallReason, HeroId, MatchId,
    MatchState, MatchStats, MatchSummary, ModeInfo, Order, PickReason, PickState, PlayerId, Role,
    Search, SlotId, Stage, Team, TickMode, Ticket, ToGame, ToHub, Token, TournamentFormat,
    TournamentId, TournamentInfo, TournamentSummary, UnitKind, WorldView,
};

use crate::camera::Camera;
use crate::net::Net;
use crate::replay_play::ReplayPlayer;

/// Where messages come from: sockets or a file.
pub enum Source {
    /// Live: the hub, and the game server of the match being played.
    Live {
        /// The hub.
        hub: Net<FromHub>,
        /// The game server of the match being played. Absent outside one.
        game: Option<Net<FromGame>>,
        /// Where that match plays and the ticket that lets this client in,
        /// to come back with when the game server's connection drops.
        play: Option<(String, Ticket)>,
    },
    /// A recorded match.
    Replay(Box<ReplayPlayer>),
}

/// One message, from the hub or from a game server.
#[derive(Clone, Debug, PartialEq)]
pub enum Heard {
    /// From the hub.
    Hub(FromHub),
    /// From the game server of the match, or from a recording of one.
    Game(FromGame),
}

impl Source {
    /// A live source over a connection to the hub.
    pub fn live(hub: Net<FromHub>) -> Source {
        Source::Live {
            hub,
            game: None,
            play: None,
        }
    }

    /// Everything due since the last frame.
    pub fn poll(&mut self, dt: f32) -> Vec<Heard> {
        match self {
            Source::Live { hub, game, .. } => {
                let mut heard: Vec<Heard> = hub.poll().into_iter().map(Heard::Hub).collect();
                if let Some(game) = game {
                    heard.extend(game.poll().into_iter().map(Heard::Game));
                }
                heard
            }
            Source::Replay(player) => player.poll(dt).into_iter().map(Heard::Game).collect(),
        }
    }

    /// Sends to the hub. A replay swallows everything.
    pub fn tell_hub(&mut self, msg: &ToHub) {
        if let Source::Live { hub, .. } = self {
            hub.send(msg);
        }
    }

    /// Sends to the game server of the match being played, when there is
    /// one.
    pub fn tell_game(&mut self, msg: &ToGame) {
        if let Source::Live {
            game: Some(game), ..
        } = self
        {
            game.send(msg);
        }
    }

    /// Goes to the game server a match plays on and joins it with the
    /// ticket. Returns whether the game server could be reached.
    pub fn enter_game(&mut self, addr: &str, ticket: Ticket) -> bool {
        let Source::Live { game, play, .. } = self else {
            return false;
        };
        *game = join_game_server(addr, ticket);
        *play = Some((addr.to_string(), ticket));
        game.is_some()
    }

    /// Joins the game server once more with the ticket last given. Comes
    /// back once for each ticket; returns whether it did.
    pub fn rejoin_game(&mut self) -> bool {
        let Source::Live { game, play, .. } = self else {
            return false;
        };
        let Some((addr, ticket)) = play.take() else {
            return false;
        };
        *game = join_game_server(&addr, ticket);
        game.is_some()
    }

    /// Hangs up on the game server.
    pub fn leave_game(&mut self) {
        if let Source::Live { game, play, .. } = self {
            *game = None;
            *play = None;
        }
    }

    /// Whether a game server is connected and listening.
    pub fn in_game(&self) -> bool {
        matches!(self, Source::Live { game: Some(game), .. } if !game.is_closed())
    }

    /// Whether the hub has hung up.
    pub fn hub_hung_up(&self) -> bool {
        matches!(self, Source::Live { hub, .. } if hub.is_closed())
    }

    /// Whether the game server of the match being played has hung up.
    pub fn game_hung_up(&self) -> bool {
        matches!(self, Source::Live { game: Some(game), .. } if game.is_closed())
    }
}

/// A connection to a game server that has been shown the ticket. Absent
/// when the game server could not be reached.
fn join_game_server(addr: &str, ticket: Ticket) -> Option<Net<FromGame>> {
    let mut net = Net::connect(addr).ok()?;
    net.send(&ToGame::Join { ticket });
    Some(net)
}

/// One unit's latest order, kept to be drawn over the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShownOrder {
    /// Which unit it was for.
    pub unit: EntityId,
    /// The tick it was given on, which decides how faded it is drawn.
    pub tick: u32,
    /// The order itself.
    pub order: Order,
}

/// Which screen the client is on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Typing a login and a password.
    SignIn,
    /// The lists of matches, tournaments and recordings.
    Hall,
    /// One tournament: its teams, its settings and its bracket.
    Cup,
    /// In a match that gathers, waiting for its seats and its start.
    Lobby,
    /// In a match whose heroes are being picked on its game server.
    Picking,
    /// The match itself, including its end banner.
    Playing,
}

/// A name the client is waiting to be typed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Asking {
    /// What to call a new match.
    NewMatch,
    /// What to call a new tournament.
    NewCup,
    /// Which team to join a tournament in.
    JoinTeam(TournamentId),
}

impl Asking {
    /// The line shown over the box.
    pub fn prompt(&self) -> &'static str {
        match self {
            Asking::NewMatch => "name the match",
            Asking::NewCup => "name the cup",
            Asking::JoinTeam(_) => "team name: a new one makes a team, a known one joins it",
        }
    }
}

/// Which list the hall shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HallTab {
    /// Matches open, playing, or lately ended.
    Matches,
    /// Tournaments.
    Tournaments,
    /// The archive, newest first.
    Replays,
}

/// Which box on the sign-in screen the keys type into.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The login.
    Login,
    /// The password.
    Password,
}

/// A replay on its way from the server to a file.
pub struct Fetching {
    /// Which match.
    pub id: MatchId,
    /// Where it is being written.
    pub path: PathBuf,
    /// How many bytes have landed.
    pub got: u64,
    /// The file, while it is open.
    file: Option<std::io::BufWriter<std::fs::File>>,
}

impl Fetching {
    /// Opens the file a replay will be written to.
    pub fn open(id: MatchId) -> Fetching {
        let path = std::env::temp_dir().join(format!("bota-replay-{}.brp", id.0));
        let file = std::fs::File::create(&path)
            .ok()
            .map(std::io::BufWriter::new);
        Fetching {
            id,
            path,
            got: 0,
            file,
        }
    }

    /// Appends what arrived.
    pub fn take(&mut self, bytes: &[u8]) {
        use std::io::Write;
        if let Some(file) = self.file.as_mut()
            && file.write_all(bytes).is_ok()
        {
            self.got += bytes.len() as u64;
        }
    }

    /// Closes the file and hands back where it is.
    pub fn finish(mut self) -> Option<PathBuf> {
        use std::io::Write;
        let mut file = self.file.take()?;
        file.flush().ok()?;
        drop(file);
        Some(self.path)
    }
}

/// What the camera is carried by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pin {
    /// A seat's hero, whichever body it is standing in.
    ///
    /// A hero that dies comes back as a new entity, and a camera pinned to
    /// the body rather than the seat would be left behind by that.
    Hero(SlotId),
    /// One unit, for as long as it stands.
    Unit(EntityId),
}

/// Keeps the last state of every unit a seat owns.
///
/// Only what a seat owns is kept, which bounds this to a handful of entries:
/// an enemy hero out of sight is worth remembering, the creep wave it walked
/// past is not. A seat stands in one body at a time, so a body it has left
/// for a new one is dropped rather than piling up.
pub fn remember(seen: &mut Vec<(u32, bota_proto::UnitView)>, view: &WorldView) {
    for unit in view.units.iter().filter(|unit| unit.owner.is_some()) {
        match seen.iter_mut().find(|(_, held)| held.id == unit.id) {
            Some(held) => *held = (view.tick, unit.clone()),
            None => seen.push((view.tick, unit.clone())),
        }
    }
    seen.retain(|(_, held)| {
        view.units.iter().any(|unit| unit.id == held.id)
            || !view.units.iter().any(|unit| {
                unit.owner == held.owner && unit.kind == held.kind && unit.id != held.id
            })
    });
}

/// What is known of a unit, and how many ticks ago that was true.
pub fn known_in<'a>(
    seen: &'a [(u32, bota_proto::UnitView)],
    view: &'a WorldView,
    id: EntityId,
) -> Option<(&'a bota_proto::UnitView, u32)> {
    if let Some(live) = view.units.iter().find(|unit| unit.id == id) {
        return Some((live, 0));
    }
    seen.iter()
        .find(|(_, held)| held.id == id)
        .map(|(tick, held)| (held, view.tick.saturating_sub(*tick)))
}

/// The body a seat last stood in, whether or not it still stands: the one the
/// wire names, else the last one seen.
pub fn body_in(
    seen: &[(u32, bota_proto::UnitView)],
    view: &WorldView,
    slot: SlotId,
) -> Option<EntityId> {
    if let Some(unit) = view
        .players
        .iter()
        .find(|p| p.slot == slot)
        .and_then(|p| p.unit)
    {
        return Some(unit);
    }
    seen.iter()
        .find(|(_, held)| held.owner == Some(slot) && held.kind == UnitKind::Hero)
        .map(|(_, held)| held.id)
}

/// What was last reached for, so a second reach for the same thing counts as
/// a pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tap {
    /// The key for one's own hero.
    Hero,
    /// The key for one's own courier.
    Courier,
    /// A unit, clicked in the world.
    Unit(EntityId),
    /// A seat, clicked in the top bar.
    Seat(SlotId),
}

/// How long after one press a second counts as a double.
pub const DOUBLE_TAP: f32 = 0.35;

/// Whether reaching for something makes a pair with what was reached for
/// last, and what is worth remembering afterwards.
///
/// A pair is spent when it is made: three presses are one pair and one
/// single, not two pairs.
pub fn tap_again(last: Option<(Tap, f32)>, what: Tap) -> (bool, Option<(Tap, f32)>) {
    if last.map(|(seen, _)| seen) == Some(what) {
        (true, None)
    } else {
        (false, Some((what, DOUBLE_TAP)))
    }
}

/// What a refusal outside a match says, in plain words.
pub fn hall_refusal(reason: HallReason) -> &'static str {
    use HallReason as Why;
    match reason {
        Why::NotSignedIn => "say who you are first",
        Why::BadName => "that name will not do",
        Why::BadPassword => "that password will not do",
        Why::LoginTaken => "that login is taken",
        Why::NoSuchLogin => "no such login",
        Why::WrongPassword => "wrong password",
        Why::UnknownToken => "that session has expired",
        Why::SignedInElsewhere => "signed in from somewhere else",
        Why::GuestsCannotPlay => "guests may only watch",
        Why::NoSuchMode => "no such mode",
        Why::NoSuchMatch => "no such match",
        Why::MatchFull => "every seat is taken",
        Why::NotGathering => "that match has started",
        Why::NotPlaying => "that match is not under way",
        Why::NotAllHere => "every seat needs a player who is here",
        Why::MatchInProgress => "you have a match in progress",
        Why::TooManySeats => "the mode does not give one player that many seats",
        Why::NotHost => "only the host may do that",
        Why::NoSeat => "you hold no seat there",
        Why::SeatTaken => "that seat is taken",
        Why::SeatsAcrossTeams => "your seats are all on one side",
        Why::SeatsFixed => "the seats are set by the tournament",
        Why::NotInMatch => "you are in no match",
        Why::NoSuchTournament => "no such tournament",
        Why::TournamentStarted => "that tournament has started",
        Why::NotOrganizer => "only the organizer may do that",
        Why::TeamFull => "that team is full",
        Why::NotInTournament => "you are in no tournament",
        Why::TooFewTeams => "two full teams are needed",
        Why::TournamentRunning => "its games have started",
        Why::InTournament => "you are in a tournament already",
        Why::NoSuchReplay => "no recording of that match",
        Why::BadSeries => "a series is one, three, five or seven games",
    }
}

/// What a game server's refusal says, in plain words.
pub fn game_refusal(reason: GameReason) -> &'static str {
    match reason {
        GameReason::UnknownTicket => "the game server does not know this ticket",
        GameReason::NotJoined => "the game server was not joined",
    }
}

/// What a refused pick says, in plain words.
pub fn pick_refusal(reason: PickReason) -> &'static str {
    match reason {
        PickReason::NotYourSeat => "that seat is not yours",
        PickReason::AlreadyPicked => "your hero is picked for good",
        PickReason::NoSuchHero => "no such hero",
        PickReason::NotPicking => "heroes are not being picked",
    }
}

/// What a refused order says, in plain words.
pub fn refusal(reason: bota_proto::RejectReason) -> &'static str {
    use bota_proto::RejectReason as Why;
    match reason {
        Why::NotYourSlot => "that seat is not yours",
        Why::HeroDead => "your hero is not standing",
        Why::UnknownTarget => "no such target",
        Why::WrongTargetKind => "not aimed at what it takes",
        Why::OnCooldown => "still on cooldown",
        Why::OutOfRange => "out of range",
        Why::NotEnoughMana => "not enough mana",
        Why::NotEnoughGold => "not enough gold",
        Why::EmptySlot => "that slot is empty",
        Why::NotCastable => "it works on its own",
        Why::NotLearned => "no points in it yet",
        Why::NoCharges => "no charges left",
        Why::NotReady => "it is not working yet",
        Why::NotYourUnit => "you do not drive that unit",
        Why::UnknownItem => "the shop does not sell that",
        Why::CannotLevelUp => "no skill point for it",
        Why::NotAtShop => "only at the shop",
        Why::InventoryFull => "no room for it",
        Why::Disabled => "you cannot act right now",
        Why::NotPlaying => "the match is not running",
        Why::NotYourItem => "not yours to sell",
        Why::ClosedGround => "nothing can lie there",
        Why::NotInBag => "not carried in the bag",
        Why::TooManyOrders => "too many orders at once",
    }
}

/// What a floater tells of, which decides how it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloaterKind {
    /// Health taken off by a plain blow.
    Damage,
    /// Health taken off by a critical strike.
    Crit,
    /// An attack that missed.
    Miss,
    /// A level gained.
    Level,
}

/// A damage number floating off a unit.
pub struct Floater {
    /// The text shown.
    pub text: String,
    /// What it tells of.
    pub kind: FloaterKind,
    /// World position it rises from.
    pub world: (f32, f32),
    /// Seconds since it appeared.
    pub age: f32,
}

/// A token as 64 hexadecimal digits, the way a session file keeps it.
pub fn token_text(token: &Token) -> String {
    token.0.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A token read back from 64 hexadecimal digits. Absent for anything else.
pub fn token_from_text(text: &str) -> Option<Token> {
    let digits = text.trim();
    if digits.len() != 64 || !digits.is_ascii() {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (at, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digits[at * 2..at * 2 + 2], 16).ok()?;
    }
    Some(Token(bytes))
}

/// A question waiting for a yes or a no before something is done that
/// cannot be taken back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirm {
    /// Giving every seat of a match being played up for good.
    Abandon(MatchId),
}

impl Confirm {
    /// Its title, its lines, and what its yes says.
    pub fn words(&self) -> (&'static str, &'static [&'static str], &'static str) {
        match self {
            Confirm::Abandon(_) => (
                "Abandon the match?",
                &[
                    "You cannot come back to it.",
                    "A side with nobody left on it loses.",
                ],
                "Abandon",
            ),
        }
    }
}

/// One line of the kill feed.
pub struct FeedLine {
    /// The text shown.
    pub text: String,
    /// Seconds since it appeared.
    pub age: f32,
}

/// Most chat lines kept; older ones are dropped.
pub const CHAT_KEPT: usize = 100;

/// One line of the chat.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatLine {
    /// Who said it: a login, and the side it plays for if any. Absent for a
    /// line from the server itself.
    pub from: Option<(String, Option<Team>)>,
    /// Whether it was said to one side alone.
    pub team: bool,
    /// What was said.
    pub text: String,
    /// Seconds since it came.
    pub age: f32,
}

/// A chat line being typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    /// What has been typed so far.
    pub text: String,
    /// Whether it goes to one's own side alone.
    pub team: bool,
}

/// The whole client.
pub struct App {
    /// Sockets or file.
    pub source: Source,
    /// Whether the hub hung up on us.
    pub lost: bool,
    /// Whether the game server of the match hung up, or could not be
    /// reached, and would not take this client back.
    pub game_lost: bool,
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
    /// The slot taken up and waiting for a click to aim it, if one is.
    pub aiming: Option<crate::slots::Slot>,
    /// An item slot picked up and waiting for the destination click.
    pub held_item: Option<u8>,
    /// Whether the shop panel is open. Toggled by key or button; buying
    /// away from home lands in the stash.
    pub shop_open: bool,
    /// The catalog row the shop panel starts at.
    pub shop_scroll: usize,
    /// What the shop sells and asks for it, from MatchStart.
    pub shop: Vec<bota_proto::ShopEntry>,
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
    /// The match this client is in, as last broadcast.
    pub room: Option<MatchState>,
    /// Seat names captured from the match state for the scoreboard.
    pub names: Vec<(SlotId, String)>,
    /// The account signed in. Absent for a guest and before signing in.
    pub account: Option<AccountInfo>,
    /// The key that signs the same account in again.
    pub token: Option<Token>,
    /// Where the token is kept between runs. Absent keeps it nowhere.
    pub session_path: Option<PathBuf>,
    /// The modes the server offers.
    pub modes: Vec<ModeInfo>,
    /// The login being typed, or the one signed in with.
    pub login: String,
    /// The password being typed.
    pub password: String,
    /// Which box the sign-in screen types into.
    pub field: Field,
    /// The last refusal, shown until something else happens.
    pub refusal: Option<HallReason>,
    /// Which list the hall shows.
    pub tab: HallTab,
    /// The matches the hall last listed.
    pub hall_matches: Vec<MatchSummary>,
    /// The tournaments the hall last listed.
    pub hall_tournaments: Vec<TournamentSummary>,
    /// The page of the archive last asked for.
    pub replays: Vec<MatchSummary>,
    /// Where the next page of the archive starts.
    pub more_replays: Option<MatchId>,
    /// The tournament being watched.
    pub tournament: Option<TournamentInfo>,
    /// A replay on its way over the wire.
    pub fetching: Option<Fetching>,
    /// A recording being watched while the connection stays in the hall.
    pub watching: Option<Box<ReplayPlayer>>,
    /// How far the hall list is scrolled.
    pub hall_scroll: usize,
    /// Where this client stands in matchmaking, as last told.
    pub search: Search,
    /// Seconds this client has waited in the queue, counted here.
    pub searched_for: f32,
    /// Seconds since the hub last said where this client stands, counted
    /// here: what has run off the times it gave since.
    pub search_age: f32,
    /// Seconds the match found in the queue gave to accept it when first
    /// told of.
    pub found_span: f32,
    /// Where the picking of heroes stands, as the game server last said.
    /// Absent outside it.
    pub picking: Option<PickState>,
    /// The hero last selected here for this client's seat while heroes are
    /// picked. Absent until one is, and outside the picking.
    pub chosen: Option<HeroId>,
    /// Seconds since the game server last said how the picking stands,
    /// counted here: what has run off its time to pick since.
    pub pick_age: f32,
    /// Which mode the hall's buttons will use.
    pub mode_at: usize,
    /// The format a new tournament will be run under.
    pub cup_format: TournamentFormat,
    /// The series length a new tournament will use.
    pub cup_best_of: u8,
    /// What the client is waiting to be typed, if anything.
    pub asking: Option<Asking>,
    /// What has been typed so far.
    pub entry: String,
    /// The question waiting for a yes or a no, if one is.
    pub confirm: Option<Confirm>,
    /// What to ask for as soon as the server signs this client in.
    pub errand: Option<ToHub>,
    /// Whether a greeting has been sent and not yet answered.
    pub greeted: bool,
    /// The freshest state of the world.
    pub view: Option<WorldView>,
    /// The state one snapshot ago, for naming what died.
    pub prev_view: Option<WorldView>,
    /// The last state seen of every unit a seat owns, and the tick it was
    /// seen on.
    ///
    /// Only what a seat owns is kept, which bounds this to a handful of
    /// entries: an enemy hero out of sight is worth remembering, the creep
    /// wave it walked past is not.
    pub seen: Vec<(u32, bota_proto::UnitView)>,
    /// The eye.
    pub camera: Camera,
    /// Whether the next left click is an attack-move.
    pub attack_move_armed: bool,
    /// The unit picked, if one is. Nothing picked falls back to one's own
    /// hero, so orders always have somewhere to go.
    pub selected: Option<EntityId>,
    /// What the camera is pinned to. Nothing means a free camera.
    pub pinned: Option<Pin>,
    /// What was last reached for, and how long is left to make it a pair.
    pub tapped: Option<(Tap, f32)>,
    /// Whether the camera has been put on one's own hero once, at the start.
    pub found_my_hero: bool,
    /// The latest order of every unit it is known for: this seat's own in a
    /// live match, whatever the server tells a spectator, everybody's in a
    /// replay.
    pub shown_orders: Vec<ShownOrder>,
    /// The seat whose eyes a spectator has asked to watch through.
    pub eyes: Option<SlotId>,
    /// Damage numbers in flight.
    pub floaters: Vec<Floater>,
    /// Kill feed lines.
    pub feed: Vec<FeedLine>,
    /// The last order a slot sent and which slot it was, so a refusal of
    /// that order can be shown on the slot that earned it.
    pub aimed_from: Option<(bota_proto::UnitOrder, crate::slots::Slot)>,
    /// The slot whose order was last refused, and the seconds left on it.
    pub refused: Option<(crate::slots::Slot, f32)>,
    /// The last rejected order, shown briefly: text and seconds left.
    pub reject: Option<(String, f32)>,
    /// The chat heard where this client is, oldest first.
    pub chat: Vec<ChatLine>,
    /// The chat line being typed. Absent while none is.
    pub draft: Option<Draft>,
    /// The end of the match, once it came.
    pub over: Option<(Team, MatchStats)>,
    /// Set when the window should close.
    pub quit: bool,
}

impl App {
    /// A client over a source, before anything has been heard.
    ///
    /// A live client starts at the sign-in screen; a recording starts where
    /// its first snapshot puts it.
    pub fn new(source: Source) -> App {
        let phase = match source {
            Source::Live { .. } => Phase::SignIn,
            Source::Replay(_) => Phase::Lobby,
        };
        App {
            source,
            lost: false,
            game_lost: false,
            phase,
            me: None,
            my_slot: None,
            tick_rate: 30,
            pregame_ticks: 0,
            aiming: None,
            held_item: None,
            shop_open: false,
            shop_scroll: 0,
            shop: Vec::new(),
            trees: Vec::new(),
            terrain_cells: 0,
            terrain: Vec::new(),
            opaque: Vec::new(),
            mode: None,
            room: None,
            names: Vec::new(),
            account: None,
            token: None,
            session_path: None,
            modes: Vec::new(),
            login: String::new(),
            password: String::new(),
            field: Field::Login,
            refusal: None,
            tab: HallTab::Matches,
            hall_matches: Vec::new(),
            hall_tournaments: Vec::new(),
            replays: Vec::new(),
            more_replays: None,
            tournament: None,
            fetching: None,
            watching: None,
            hall_scroll: 0,
            search: Search::Idle,
            searched_for: 0.0,
            search_age: 0.0,
            found_span: 0.0,
            picking: None,
            chosen: None,
            pick_age: 0.0,
            mode_at: 0,
            cup_format: TournamentFormat::SingleElimination,
            cup_best_of: 1,
            asking: None,
            entry: String::new(),
            confirm: None,
            errand: None,
            greeted: false,
            view: None,
            prev_view: None,
            seen: Vec::new(),
            camera: Camera::over(9216.0, 9216.0),
            attack_move_armed: false,
            selected: None,
            pinned: None,
            tapped: None,
            found_my_hero: false,
            shown_orders: Vec::new(),
            eyes: None,
            floaters: Vec::new(),
            feed: Vec::new(),
            aimed_from: None,
            refused: None,
            reject: None,
            chat: Vec::new(),
            draft: None,
            over: None,
            quit: false,
        }
    }

    /// Says who this client is and waits for the answer.
    pub fn say_hello(&mut self, auth: bota_proto::Auth) {
        self.greeted = true;
        self.refusal = None;
        self.source.tell_hub(&ToHub::Hello { auth });
    }

    /// The mode the hall's buttons will use.
    pub fn chosen_mode(&self) -> Option<&ModeInfo> {
        self.modes.get(self.mode_at)
    }

    /// Waits in the queue of the chosen mode for a seat in a match.
    pub fn find_match(&mut self) {
        let Some(mode) = self.chosen_mode().map(|m| m.id) else {
            return;
        };
        self.source.tell_hub(&ToHub::FindMatch { mode, seats: 1 });
    }

    /// Leaves the queue, or turns down the match found in it.
    pub fn cancel_search(&mut self) {
        self.source.tell_hub(&ToHub::CancelSearch);
    }

    /// Accepts the match the queue found, when there is one this client has
    /// not accepted yet.
    pub fn accept_found(&mut self) {
        if let Search::Found {
            id, yours: false, ..
        } = self.search
        {
            self.source.tell_hub(&ToHub::Accept { id });
        }
    }

    /// Turns down the match the queue found, when there is one.
    pub fn decline_found(&mut self) {
        if let Search::Found { id, .. } = self.search {
            self.source.tell_hub(&ToHub::Decline { id });
        }
    }

    /// Whether this client waits in the queue or has a match found in it.
    pub fn searching(&self) -> bool {
        self.search != Search::Idle
    }

    /// Seconds left of a time the hub gave in milliseconds, `age` seconds
    /// ago. Never below zero.
    pub fn seconds_left(ms: u32, age: f32) -> f32 {
        (ms as f32 / 1000.0 - age).max(0.0)
    }

    /// Waits for a name to be typed.
    pub fn ask(&mut self, asking: Asking) {
        self.entry = match asking {
            Asking::NewMatch => format!("{}'s match", self.login),
            Asking::NewCup => format!("{}'s cup", self.login),
            Asking::JoinTeam(_) => self.login.clone(),
        };
        self.asking = Some(asking);
    }

    /// Sends whatever the typed name was for.
    pub fn confirm_entry(&mut self) {
        let (Some(asking), Some(mode)) = (self.asking.take(), self.chosen_mode().map(|m| m.id))
        else {
            self.entry.clear();
            return;
        };
        let name = std::mem::take(&mut self.entry);
        if name.trim().is_empty() {
            return;
        }
        match asking {
            Asking::NewMatch => self.source.tell_hub(&ToHub::CreateMatch {
                mode,
                name,
                seed: None,
                seats: 1,
            }),
            Asking::NewCup => self.source.tell_hub(&ToHub::CreateTournament {
                name,
                mode,
                format: self.cup_format,
                best_of: self.cup_best_of,
            }),
            Asking::JoinTeam(id) => self.source.tell_hub(&ToHub::JoinTournament {
                id,
                team: name,
                seats: 1,
            }),
        }
    }

    /// Opens the screen of one tournament and follows it.
    pub fn open_cup(&mut self, id: TournamentId) {
        self.source
            .tell_hub(&ToHub::ViewTournament { id: Some(id) });
        if self.tournament.as_ref().is_none_or(|cup| cup.id != id) {
            self.tournament = None;
        }
        self.phase = Phase::Cup;
    }

    /// Stops following a tournament and goes back to the hall.
    pub fn close_cup(&mut self) {
        self.source.tell_hub(&ToHub::ViewTournament { id: None });
        self.tournament = None;
        self.phase = Phase::Hall;
    }

    /// Asks for a page of the archive.
    pub fn ask_for_replays(&mut self, before: Option<MatchId>) {
        self.source.tell_hub(&ToHub::ListMatches {
            filter: bota_proto::MatchFilter {
                stage: Some(Stage::Over),
                tournament: None,
                account: None,
            },
            before,
            limit: 40,
        });
    }

    /// Everything due since the last frame, from the connection and from a
    /// recording being watched over it.
    pub fn pump(&mut self, dt: f32) -> Vec<Heard> {
        let mut heard = self.source.poll(dt);
        if let Some(player) = self.watching.as_mut() {
            heard.extend(player.poll(dt).into_iter().map(Heard::Game));
        }
        heard
    }

    /// Whether what is on screen comes from a recording.
    pub fn replaying(&self) -> bool {
        self.player().is_some()
    }

    /// The recording being played, from a file or fetched from the server.
    pub fn player(&self) -> Option<&ReplayPlayer> {
        match (&self.watching, &self.source) {
            (Some(player), _) => Some(player),
            (None, Source::Replay(player)) => Some(player),
            (None, Source::Live { .. }) => None,
        }
    }

    /// The same, to drive it.
    pub fn player_mut(&mut self) -> Option<&mut ReplayPlayer> {
        match (&mut self.watching, &mut self.source) {
            (Some(player), _) => Some(player),
            (None, Source::Replay(player)) => Some(player),
            (None, Source::Live { .. }) => None,
        }
    }

    /// Writes the login and token where the next run will find them.
    pub fn remember_session(&self) {
        let (Some(path), Some(token)) = (self.session_path.as_ref(), self.token) else {
            return;
        };
        let _ = std::fs::write(path, format!("{}\n{}\n", self.login, token_text(&token)));
    }

    /// Drops everything about the match this client was in and goes back to
    /// the hall.
    pub fn leave_room(&mut self) {
        self.forget_match();
        self.room = None;
        self.my_slot = None;
        self.names.clear();
        self.phase = Phase::Hall;
    }

    /// Drops what was seen and heard of the match this client was in, stops
    /// a recording being watched, and hangs up on the game server.
    fn forget_match(&mut self) {
        self.watching = None;
        self.confirm = None;
        self.picking = None;
        self.chosen = None;
        self.view = None;
        self.prev_view = None;
        self.seen.clear();
        self.shown_orders.clear();
        self.floaters.clear();
        self.feed.clear();
        self.eyes = None;
        self.selected = None;
        self.pinned = None;
        self.found_my_hero = false;
        self.over = None;
        self.shop_open = false;
        self.chat.clear();
        self.draft = None;
        self.game_lost = false;
        self.source.leave_game();
    }

    /// Whether this account holds a seat it has not given up in a match the
    /// hall lists as gathering, picking or being played. A gathering match
    /// keeps a seat for an account that is not in it only when the seat is
    /// fixed: a tournament's, or the queue's.
    pub fn holds_seat_in(&self, m: &MatchSummary) -> bool {
        let Some(account) = &self.account else {
            return false;
        };
        matches!(m.stage, Stage::Gathering | Stage::Picking | Stage::Playing)
            && m.seats
                .iter()
                .any(|seat| seat.login == account.login && !seat.abandoned)
    }

    /// The match, gathering, picking or being played, that this account
    /// holds a seat in and is not in, as the hall lists it.
    pub fn match_to_rejoin(&self) -> Option<&MatchSummary> {
        if self.room.is_some() {
            return None;
        }
        self.hall_matches.iter().find(|m| self.holds_seat_in(m))
    }

    /// Goes back to this account's seats in a match it is not in.
    pub fn rejoin(&mut self, id: MatchId) {
        self.source.tell_hub(&ToHub::JoinMatch {
            id,
            role: Role::Player { seats: 1 },
        });
    }

    /// Gives this account's seats in a match that has started up for good.
    pub fn abandon(&mut self, id: MatchId) {
        self.confirm = None;
        self.source.tell_hub(&ToHub::Abandon { id });
    }

    /// Whether this client hosts the match it is in.
    pub fn hosts(&self) -> bool {
        let (Some(room), Some(account)) = (&self.room, &self.account) else {
            return false;
        };
        room.host == Some(account.id)
    }

    /// Sends the match this client hosts off to be played.
    pub fn start_match(&mut self) {
        self.source.tell_hub(&ToHub::StartMatch);
    }

    /// The hero picked for this client's seat. Absent until one is.
    pub fn my_pick(&self) -> Option<HeroId> {
        let mine = self.my_slot?;
        self.picking
            .as_ref()?
            .seats
            .iter()
            .find(|seat| seat.slot == mine)?
            .hero
    }

    /// Whether this client holds a seat whose hero is still to be picked.
    pub fn may_pick(&self) -> bool {
        self.my_slot.is_some() && self.picking.is_some() && self.my_pick().is_none()
    }

    /// The hero this client's seat is set on and has not picked: the one
    /// last selected here, else the one the game server tells. Absent once
    /// the seat has picked.
    pub fn my_selection(&self) -> Option<HeroId> {
        if !self.may_pick() {
            return None;
        }
        let mine = self.my_slot?;
        self.chosen.or_else(|| {
            self.picking
                .as_ref()?
                .seats
                .iter()
                .find(|seat| seat.slot == mine)?
                .selected
        })
    }

    /// Sets this client's seat on a hero, for its side to see, without
    /// picking it.
    pub fn select_hero(&mut self, hero: HeroId) {
        if let Some(seat) = self.my_slot.filter(|_| self.may_pick()) {
            self.chosen = Some(hero);
            self.source.tell_game(&ToGame::Select { seat, hero });
        }
    }

    /// Picks the hero this client's seat is set on, for good. Nothing
    /// happens while it is set on none.
    pub fn pick_selected(&mut self) {
        if let Some(hero) = self.my_selection() {
            self.pick_hero(hero);
        }
    }

    /// Picks the hero of this client's seat, for good.
    pub fn pick_hero(&mut self, hero: HeroId) {
        if let Some(seat) = self.my_slot.filter(|_| self.may_pick()) {
            self.source.tell_game(&ToGame::Pick { seat, hero });
        }
    }

    /// Sends a chat line where this client is: to the game server of the
    /// match being played, else to the hub.
    pub fn say(&mut self, text: String, team: bool) {
        if self.source.in_game() {
            self.source.tell_game(&ToGame::Chat { text, team });
        } else {
            self.source.tell_hub(&ToHub::Chat { text, team });
        }
    }

    /// Keeps one line of the chat, dropping the oldest past
    /// [`CHAT_KEPT`].
    pub fn hear(&mut self, from: Option<(String, Option<Team>)>, team: bool, text: String) {
        self.chat.push(ChatLine {
            from,
            team,
            text,
            age: 0.0,
        });
        let over = self.chat.len().saturating_sub(CHAT_KEPT);
        self.chat.drain(..over);
    }

    /// Plays a recording over the connection, which stays in the hall.
    pub fn watch(&mut self, path: &std::path::Path) {
        match ReplayPlayer::load(path) {
            Ok(player) => {
                self.leave_room();
                self.watching = Some(Box::new(player));
                self.phase = Phase::Lobby;
            }
            Err(err) => self.reject = Some((format!("the replay would not open: {err}"), 4.0)),
        }
    }

    /// Stops watching a recording and goes back to the hall.
    pub fn stop_watching(&mut self) {
        self.leave_room();
    }

    /// The display name of a seat.
    pub fn seat_name(&self, slot: SlotId) -> String {
        self.names
            .iter()
            .find(|(s, _)| *s == slot)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| format!("seat {}", slot.0))
    }

    /// What is known of a unit: its live state, or the last one seen of it
    /// and how many ticks ago that was.
    pub fn known(&self, id: EntityId) -> Option<(&bota_proto::UnitView, u32)> {
        known_in(&self.seen, self.view.as_ref()?, id)
    }

    /// The body a seat last stood in, whether or not it still stands.
    pub fn body_of(&self, slot: SlotId) -> Option<EntityId> {
        body_in(&self.seen, self.view.as_ref()?, slot)
    }

    /// Our hero's unit id, standing or fallen.
    pub fn my_hero(&self) -> Option<bota_proto::EntityId> {
        let slot = self.my_slot?;
        let view = self.view.as_ref()?;
        view.players.iter().find(|p| p.slot == slot)?.unit
    }

    /// Whether our hero stands on the field.
    pub fn hero_stands(&self) -> bool {
        let (Some(hero), Some(view)) = (self.my_hero(), self.view.as_ref()) else {
            return false;
        };
        view.units.iter().any(|unit| unit.id == hero)
    }

    /// Which unit the slot panel and the keys are about.
    ///
    /// Whatever is picked, if anything is; one's own hero otherwise.
    pub fn commanded(&self) -> Option<bota_proto::EntityId> {
        match self.selected {
            Some(id) => Some(id),
            None => self.my_hero(),
        }
    }

    /// Picks a unit, and pins the camera to it when the same thing is
    /// reached for twice.
    ///
    /// Picking alone never moves the camera: what a player is looking at and
    /// what a player is commanding are two different questions.
    pub fn choose(&mut self, unit: Option<EntityId>, again: bool) {
        self.selected = unit;
        self.aiming = None;
        if again {
            self.pinned = unit.map(|id| match self.hero_seat(id) {
                Some(slot) => Pin::Hero(slot),
                None => Pin::Unit(id),
            });
        }
    }

    /// Which seat a unit is the standing hero of, if it is one.
    fn hero_seat(&self, id: EntityId) -> Option<SlotId> {
        let (unit, _) = self.known(id)?;
        (unit.kind == UnitKind::Hero)
            .then_some(unit.owner)
            .flatten()
    }

    /// Which unit the camera is carried by right now, if any still stands.
    pub fn pinned_unit(&self) -> Option<EntityId> {
        let view = self.view.as_ref()?;
        match self.pinned? {
            Pin::Hero(slot) => view.players.iter().find(|p| p.slot == slot)?.unit,
            Pin::Unit(id) => view.units.iter().find(|u| u.id == id).map(|u| u.id),
        }
    }

    /// Whether this reach for something is the second of a pair.
    pub fn tapped_twice(&mut self, what: Tap) -> bool {
        let (again, kept) = tap_again(self.tapped, what);
        self.tapped = kept;
        again
    }

    /// The seat whose gold, stash and score the panel is about.
    ///
    /// Whoever owns what is picked, and one's own seat when what is picked
    /// answers to nobody.
    pub fn panel_slot(&self) -> Option<SlotId> {
        let owner = self
            .selected
            .and_then(|id| self.known(id))
            .and_then(|(unit, _)| unit.owner);
        owner.or_else(|| self.default_slot())
    }

    /// Our own courier, while one stands.
    pub fn my_courier(&self) -> Option<bota_proto::EntityId> {
        let view = self.view.as_ref()?;
        let mine = self.my_slot?;
        view.units
            .iter()
            .find(|unit| unit.kind == bota_proto::UnitKind::Courier && unit.owner == Some(mine))
            .map(|unit| unit.id)
    }

    /// Whether a unit is one this seat drives: whatever of ours stands, and
    /// our hero standing or fallen.
    pub fn drives(&self, id: bota_proto::EntityId) -> bool {
        if self.my_hero() == Some(id) {
            return true;
        }
        let Some(view) = self.view.as_ref() else {
            return false;
        };
        view.units
            .iter()
            .find(|unit| unit.id == id)
            .is_some_and(|unit| unit.owner.is_some() && unit.owner == self.my_slot)
    }

    /// Which of our units an order goes to.
    ///
    /// Whatever of ours is selected takes it; with our own hero selected, or
    /// nothing of ours at all, the order names nobody and the server sends it
    /// to the hero.
    pub fn ordering_unit(&self) -> Option<bota_proto::EntityId> {
        let id = self.selected?;
        let view = self.view.as_ref()?;
        let mine = self.my_slot?;
        let unit = view.units.iter().find(|unit| unit.id == id)?;
        (unit.owner == Some(mine) && Some(id) != self.my_hero()).then_some(id)
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
    /// enemy or a creep can be inspected without stealing the keys. What is
    /// ours answers wherever it came from: a hero, a courier, or anything
    /// else this seat is given to drive.
    pub fn controls_selection(&self) -> bool {
        match self.selected {
            None => true,
            Some(id) => self.drives(id),
        }
    }

    /// The team whose fog this client lives under. Absent for spectators,
    /// who see everything.
    pub fn fog_team(&self) -> Option<bota_proto::Team> {
        let view = self.view.as_ref()?;
        if let Some(slot) = self.my_slot {
            return view.players.iter().find(|p| p.slot == slot).map(|p| p.team);
        }
        // A spectator's fog is whatever eyes the snapshot came through.
        view.viewer
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

    /// Whether one of the panel's fifteen item slots holds an item right now.
    pub fn item_at(&self, slot: u8) -> bool {
        self.item_in(slot).is_some()
    }

    /// The seat the bottom panel falls back to: our own, else the first one.
    pub fn default_slot(&self) -> Option<SlotId> {
        self.my_slot
            .or_else(|| Some(self.view.as_ref()?.players.first()?.slot))
    }

    /// Sends an order to what is selected. Returns it as it went out.
    pub fn send_order(&mut self, order: Order) -> Option<bota_proto::UnitOrder> {
        let unit = self.ordering_unit();
        self.send_order_to(unit, order)
    }

    /// Sends one order to a named unit, whatever is selected. Returns it as
    /// it went out.
    ///
    /// What is selected is where orders go by default; this is for the few
    /// that name their unit themselves, so that sending one does not mean
    /// looking away from the fight.
    pub fn send_order_to(
        &mut self,
        unit: Option<bota_proto::EntityId>,
        order: Order,
    ) -> Option<bota_proto::UnitOrder> {
        // An order names its unit; naming nothing means one's own hero,
        // standing or fallen.
        let Some(unit) = unit.or_else(|| self.my_hero()) else {
            self.reject = Some((refusal(bota_proto::RejectReason::HeroDead).to_string(), 2.5));
            return None;
        };
        if let Some(view) = self.view.as_ref() {
            self.note_order(unit, view.tick, order);
        }
        let given = bota_proto::UnitOrder { unit, order };
        self.source.tell_game(&ToGame::Orders {
            orders: vec![given],
        });
        Some(given)
    }

    /// Remembers one unit's order for the overlay, replacing its last one.
    pub fn note_order(&mut self, unit: EntityId, tick: u32, order: Order) {
        self.shown_orders.retain(|shown| shown.unit != unit);
        self.shown_orders.push(ShownOrder { unit, tick, order });
    }

    /// Which slot of our courier holds one ability.
    pub fn courier_slot(&self, ability: u16) -> Option<u8> {
        let view = self.view.as_ref()?;
        let courier = self.my_courier()?;
        let unit = view.units.iter().find(|unit| unit.id == courier)?;
        unit.abilities
            .iter()
            .position(|held| held.id.0 == ability)
            .map(|slot| slot as u8)
    }

    /// Applies one message, from the hub or from a game server.
    pub fn handle(&mut self, heard: Heard) {
        match heard {
            Heard::Hub(msg) => self.handle_hub(msg),
            Heard::Game(msg) => self.handle_game(msg),
        }
    }

    /// Applies one message from the hub.
    fn handle_hub(&mut self, msg: FromHub) {
        match msg {
            FromHub::Welcome {
                player_id,
                account,
                token,
                modes,
            } => {
                self.me = Some(player_id);
                self.modes = modes;
                self.refusal = None;
                if let Some(account) = &account {
                    self.login = account.login.clone();
                }
                self.account = account;
                self.token = token;
                self.greeted = false;
                self.password.clear();
                self.remember_session();
                // A match state follows for anybody the server put straight
                // back into one; until then this connection is in the hall.
                if self.phase == Phase::SignIn {
                    self.phase = Phase::Hall;
                }
                if let Some(errand) = self.errand.take() {
                    self.source.tell_hub(&errand);
                }
            }
            FromHub::MatchState { state } => {
                // News of another match is a move into it, such as a cup's
                // next game: nothing of the last one comes along, the hall's
                // chat included.
                if self.room.as_ref().map(|room| room.id) != Some(state.id) {
                    self.forget_match();
                }
                self.names = state
                    .slots
                    .iter()
                    .filter(|s| !s.login.is_empty())
                    .map(|s| (s.slot, s.login.clone()))
                    .collect();
                // A person at a client holds one seat; the first is the one
                // the screen is about.
                self.my_slot = state.yours.first().copied();
                self.phase = match state.stage {
                    Stage::Gathering => Phase::Lobby,
                    _ => self.phase,
                };
                self.room = Some(state);
            }
            FromHub::Hall {
                matches,
                tournaments,
            } => {
                self.hall_matches = matches;
                self.hall_tournaments = tournaments;
                if matches!(self.phase, Phase::Lobby | Phase::Picking | Phase::Playing) {
                    self.leave_room();
                }
                if self.phase == Phase::SignIn {
                    self.phase = Phase::Hall;
                }
            }
            FromHub::Matches { page, before } => {
                self.replays = page;
                self.more_replays = before;
            }
            FromHub::TournamentState { state } => self.tournament = Some(state),
            FromHub::Refused {
                reason: HallReason::NoSuchTournament,
            } if self.phase == Phase::Cup => {
                // The cup was taken down under us.
                self.reject = Some((hall_refusal(HallReason::NoSuchTournament).to_string(), 4.0));
                self.tournament = None;
                self.phase = Phase::Hall;
            }
            FromHub::Refused { reason } => {
                self.refusal = Some(reason);
                self.reject = Some((hall_refusal(reason).to_string(), 4.0));
                if self.greeted {
                    self.greeted = false;
                    self.errand = None;
                    self.phase = Phase::SignIn;
                    if reason == HallReason::UnknownToken {
                        self.token = None;
                    }
                }
            }
            FromHub::ReplayChunk {
                match_id, bytes, ..
            } => {
                let held = self
                    .fetching
                    .get_or_insert_with(|| Fetching::open(match_id));
                if held.id == match_id {
                    held.take(&bytes);
                }
            }
            FromHub::ReplayEnd { match_id, .. } => {
                if let Some(path) = self
                    .fetching
                    .take()
                    .filter(|held| held.id == match_id)
                    .and_then(Fetching::finish)
                {
                    self.watch(&path);
                }
            }
            FromHub::Play { addr, ticket, .. } => {
                self.game_lost = !self.source.enter_game(&addr, ticket);
                if self.game_lost {
                    self.feed.push(FeedLine {
                        text: format!("the game server at {addr} could not be reached"),
                        age: 0.0,
                    });
                }
            }
            FromHub::Chat {
                from,
                side,
                team,
                text,
            } => self.hear(Some((from, side)), team, text),
            FromHub::Notice { text } => self.hear(None, false, text),
            FromHub::Search { search } => {
                let started = !matches!(self.search, Search::Queued { .. } | Search::Found { .. });
                if started {
                    self.searched_for = 0.0;
                }
                let first_word = match (&self.search, &search) {
                    (Search::Found { id: was, .. }, Search::Found { id, .. }) => was != id,
                    (_, Search::Found { .. }) => true,
                    _ => false,
                };
                if let Search::Found { ms_left, .. } = &search
                    && first_word
                {
                    self.found_span = *ms_left as f32 / 1000.0;
                }
                self.search_age = 0.0;
                self.search = search;
            }
        }
    }

    /// Applies one message from the game server of the match, or from a
    /// recording of one.
    fn handle_game(&mut self, msg: FromGame) {
        match msg {
            FromGame::Picking { state } => {
                self.picking = Some(state);
                self.pick_age = 0.0;
                self.phase = Phase::Picking;
            }
            FromGame::PickRefused { reason, .. } => {
                self.reject = Some((pick_refusal(reason).to_string(), 2.5));
            }
            FromGame::Orders { tick, orders } => {
                for given in orders {
                    self.note_order(given.unit, tick, given.order);
                }
            }
            FromGame::MatchStart { info } => {
                self.picking = None;
                self.chosen = None;
                self.shown_orders.clear();
                self.eyes = None;
                self.mode = Some(info.mode);
                self.tick_rate = info.tick_rate;
                self.pregame_ticks = info.pregame_ticks;
                self.shop = info.shop.clone();
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
            FromGame::Snapshot { view } => {
                if self.mode == Some(TickMode::Lockstep) && self.my_slot.is_some() {
                    self.source.tell_game(&ToGame::Ack { tick: view.tick });
                }
                remember(&mut self.seen, &view);
                self.prev_view = self.view.replace(view);
            }
            FromGame::Events { events, .. } => {
                for event in events {
                    self.absorb_event(event);
                }
            }
            FromGame::OrderRejected { order, reason } => {
                self.reject = Some((refusal(reason).to_string(), 2.5));
                // The slot that earned it lights up, so the answer is where
                // the press was rather than only in a line of text.
                self.refused = self
                    .aimed_from
                    .filter(|(sent, _)| *sent == order)
                    .map(|(_, slot)| (slot, 1.2));
            }
            FromGame::MatchOver { winner, stats } => {
                self.over = Some((winner, stats));
            }
            FromGame::ParticipantLeft { slots, .. } => {
                for slot in slots {
                    self.feed.push(FeedLine {
                        text: format!("{} disconnected", self.seat_name(slot)),
                        age: 0.0,
                    });
                }
            }
            FromGame::Chat {
                from,
                side,
                team,
                text,
            } => self.hear(Some((from, side)), team, text),
            FromGame::Notice { text } => self.hear(None, false, text),
            FromGame::Refused { reason } => {
                self.source.leave_game();
                self.game_lost = true;
                self.feed.push(FeedLine {
                    text: game_refusal(reason).to_string(),
                    age: 0.0,
                });
            }
        }
    }

    fn absorb_event(&mut self, event: EventKind) {
        match event {
            EventKind::Damaged {
                target,
                amount,
                crit,
                ..
            } => {
                if let Some(pos) = self.unit_pos(target) {
                    let (text, kind) = if crit {
                        (format!("{amount}!"), FloaterKind::Crit)
                    } else {
                        (format!("{amount}"), FloaterKind::Damage)
                    };
                    self.floaters.push(Floater {
                        text,
                        kind,
                        world: pos,
                        age: 0.0,
                    });
                }
            }
            EventKind::Missed { target, .. } => {
                if let Some(pos) = self.unit_pos(target) {
                    self.floaters.push(Floater {
                        text: "miss".to_string(),
                        kind: FloaterKind::Miss,
                        world: pos,
                        age: 0.0,
                    });
                }
            }
            EventKind::Died {
                unit, killer, gold, ..
            } => {
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
                    Some(by) if gold > 0 => format!("{victim} was slain by {by} for {gold} gold"),
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
                        kind: FloaterKind::Level,
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

    /// Notices a dead socket once and says so. A game server that hung up
    /// before the match was over is joined once more with the same ticket.
    pub fn check_connection(&mut self) {
        if self.lost || self.over.is_some() {
            return;
        }
        if self.source.hub_hung_up() {
            self.lost = true;
            self.feed.push(FeedLine {
                text: "connection to the hub lost".to_string(),
                age: 0.0,
            });
            return;
        }
        if self.source.game_hung_up() && !self.source.rejoin_game() {
            self.source.leave_game();
            self.game_lost = true;
            self.feed.push(FeedLine {
                text: "connection to the game server lost".to_string(),
                age: 0.0,
            });
        }
    }

    /// Ages and expires the transient effects, and runs the clocks the hub
    /// gave.
    pub fn tick_effects(&mut self, dt: f32) {
        self.searched_for += dt;
        self.search_age += dt;
        self.pick_age += dt;
        for f in &mut self.floaters {
            f.age += dt;
        }
        self.floaters.retain(|f| f.age < 1.2);
        for l in &mut self.feed {
            l.age += dt;
        }
        self.feed.retain(|l| l.age < 8.0);
        for l in &mut self.chat {
            l.age += dt;
        }
        if let Some((_, left)) = &mut self.reject {
            *left -= dt;
            if *left <= 0.0 {
                self.reject = None;
            }
        }
        if let Some((_, left)) = &mut self.refused {
            *left -= dt;
            if *left <= 0.0 {
                self.refused = None;
            }
        }
        if let Some((_, left)) = &mut self.tapped {
            *left -= dt;
            if *left <= 0.0 {
                self.tapped = None;
            }
        }
    }
}
