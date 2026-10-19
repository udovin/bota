//! The hub thread: every connection, where each one is, and what it may do.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bota_proto::{
    AccountId, AccountInfo, Assign, Auth, FromHub, GameInfo, HallReason, MAX_CHAT_CHARS,
    MatchFilter, MatchId, MatchSetup, MatchStats, ModeId, Nonce, Pass, Pick, PlayerId, Report,
    Role, Search, SlotId, Stage, Team, Ticket, ToHub, Token, TournamentFormat, TournamentId,
    TournamentInfo, TournamentRef, TournamentStage,
};

use crate::archive::{Archive, FetchJob, Fetcher};
use crate::commands::{Deed, read_command};
use crate::hub::{
    Accounts, Found, HubOpts, Match, Member, Mode, Place, Searcher, ServerLink, Session, Tourney,
    Who, best_of_is_valid, bracket, fill_sides, guest_name_is_valid,
};
use crate::keys::{Prover, fresh, fresh_u64, prove, proves};
use crate::net::{Connection, NetEvent, Outbox, accept_loop};

/// Refused sign-ins on one connection before it is closed.
pub const SIGNIN_FAILURES: u8 = 5;
/// Most characters in a match or tournament name.
pub const TITLE_MAX: usize = 48;
/// Ended tournaments the hall keeps listing.
pub const FINISHED_TOURNAMENTS_SHOWN: usize = 10;

/// Who a connection speaks as where it is.
struct Voice {
    /// The login its lines are signed with.
    login: String,
    /// The side it plays for. Absent for a spectator and in the hall.
    side: Option<Team>,
    /// Where it is heard: the index of its match, absent for the hall.
    at: Option<usize>,
}

/// Everything that reaches the hub thread.
pub enum HubEvent {
    /// From a participant's socket.
    Client(NetEvent<ToHub>),
    /// From a game server's link.
    Server(NetEvent<Report>),
}

impl From<NetEvent<ToHub>> for HubEvent {
    fn from(event: NetEvent<ToHub>) -> HubEvent {
        HubEvent::Client(event)
    }
}

impl From<NetEvent<Report>> for HubEvent {
    fn from(event: NetEvent<Report>) -> HubEvent {
        HubEvent::Server(event)
    }
}

/// Runs the hub on two already-bound listeners: one for participants, one
/// for game servers. Returns only when the event channel is gone, which is
/// never while the accept threads live.
pub fn serve(listener: TcpListener, servers: TcpListener, opts: HubOpts) -> std::io::Result<()> {
    let (tx, rx) = channel();
    let clients_tx = tx.clone();
    thread::spawn(move || accept_loop::<ToHub, HubEvent>(listener, clients_tx));
    let servers_tx = tx.clone();
    thread::spawn(move || accept_loop::<Report, HubEvent>(servers, servers_tx));
    let mut hub = Hub::open(opts, tx, rx)?;
    hub.run();
    Ok(())
}

/// The hub's state, owned by its thread.
pub struct Hub {
    opts: HubOpts,
    rx: Receiver<HubEvent>,
    tx: Sender<HubEvent>,
    conns: Vec<Connection>,
    sessions: Vec<Session>,
    links: Vec<ServerLink>,
    accounts: Accounts,
    archive: Archive,
    fetcher: Fetcher,
    matches: Vec<Match>,
    tournaments: Vec<Tourney>,
    /// Connections waiting in the matchmaking queue.
    queue: Vec<Searcher>,
    /// Matches found in the queue, waiting to be accepted.
    found: Vec<Found>,
    /// The place in the queue the next searcher takes.
    next_turn: u64,
    next_player: u32,
    hall_dirty: bool,
    hall_pushed: Option<Instant>,
}

impl Hub {
    /// Opens the data directory and reads what is in it.
    pub fn open(
        opts: HubOpts,
        tx: Sender<HubEvent>,
        rx: Receiver<HubEvent>,
    ) -> std::io::Result<Hub> {
        let archive = Archive::open(&opts.data)?;
        let accounts = Accounts::open(&archive.accounts_path(), opts.hash_cost, opts.token_life)?;
        let tournaments = archive
            .tournaments()
            .iter()
            .cloned()
            .map(|info| Tourney::from_info(info, &accounts))
            .collect();
        let mut hub = Hub {
            opts,
            rx,
            tx,
            conns: Vec::new(),
            sessions: Vec::new(),
            links: Vec::new(),
            accounts,
            archive,
            fetcher: Fetcher::spawn(),
            matches: Vec::new(),
            tournaments,
            queue: Vec::new(),
            found: Vec::new(),
            next_turn: 1,
            next_player: 1,
            hall_dirty: false,
            hall_pushed: None,
        };
        hub.resume_tournaments();
        Ok(hub)
    }

    /// Handles events until the channel closes.
    pub fn run(&mut self) {
        loop {
            match self.rx.recv_timeout(Duration::from_millis(250)) {
                Ok(HubEvent::Client(event)) => self.on_net(event),
                Ok(HubEvent::Server(event)) => self.on_server(event),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            self.housekeep();
        }
    }

    // ----- game servers -----

    fn on_server(&mut self, event: NetEvent<Report>) {
        match event {
            NetEvent::Connected(stream) => {
                let id = PlayerId(self.next_player);
                self.next_player += 1;
                let conn = Connection::spawn::<Report, HubEvent>(id, stream, self.tx.clone());
                let challenge = Nonce(fresh());
                conn.send(&Assign::Challenge { nonce: challenge });
                self.links.push(ServerLink {
                    conn,
                    challenge,
                    offer: None,
                    running: Vec::new(),
                });
            }
            NetEvent::Disconnected(id) => self.drop_server(id),
            NetEvent::Msg(id, report) => self.on_report(id, report),
        }
    }

    fn on_report(&mut self, id: PlayerId, report: Report) {
        let Some(at) = self.links.iter().position(|link| link.conn.id == id) else {
            return;
        };
        match report {
            Report::Register {
                proof,
                nonce,
                addr,
                capacity,
            } => {
                let link = &mut self.links[at];
                let held = link.offer.is_none()
                    && proves(&self.opts.key, Prover::Game, &link.challenge, &proof);
                if !held {
                    return self.drop_server(id);
                }
                link.conn.send(&Assign::Accepted {
                    proof: prove(&self.opts.key, Prover::Hub, &nonce),
                });
                link.offer = Some((addr, capacity));
                self.start_waiting();
            }
            // Nothing but the handshake is taken from a game server that has
            // not proved itself.
            _ if self.links[at].offer.is_none() => self.drop_server(id),
            Report::Picked { match_id, picks } => self.on_picked(id, match_id, &picks),
            Report::Replay { match_id, bytes } => {
                if let Some(m) = self
                    .matches
                    .iter_mut()
                    .find(|m| m.id == match_id && m.server == Some(id))
                    && let Some(file) = m.replay.as_mut()
                    && file.write_all(&bytes).is_err()
                {
                    m.replay = None;
                }
            }
            Report::Ended {
                match_id,
                winner,
                stats,
                duration,
            } => {
                let ours = self
                    .matches
                    .iter()
                    .any(|m| m.id == match_id && m.server == Some(id));
                if ours {
                    self.on_ended(match_id, winner, stats, duration);
                }
            }
        }
    }

    /// Forgets a game server, and every match it was playing ends with no
    /// winner.
    fn drop_server(&mut self, id: PlayerId) {
        let Some(at) = self.links.iter().position(|link| link.conn.id == id) else {
            return;
        };
        let gone = self.links.remove(at);
        gone.conn.close();
        for match_id in gone.running {
            let stats = MatchStats {
                duration: 0,
                slots: Vec::new(),
            };
            self.on_ended(match_id, None, stats, 0);
        }
    }

    /// Starts every match that waits for a game server with room. A match
    /// anybody created plays only with every seat's holder in it: one whose
    /// seat came open while it waited goes back to its host to start again.
    fn start_waiting(&mut self) {
        let waiting: Vec<MatchId> = self
            .matches
            .iter()
            .filter(|m| m.waiting_server && m.stage == Stage::Gathering)
            .map(|m| m.id)
            .collect();
        for id in waiting {
            let Some(at) = self.match_at(id) else {
                continue;
            };
            let m = &mut self.matches[at];
            if m.seats_fixed() || m.roster.all_present() {
                self.start_match(id);
                continue;
            }
            m.waiting_server = false;
            for player in m.players() {
                self.notice(
                    player,
                    "a seat came open, and the match waits for its host again",
                );
            }
        }
    }

    /// Every seat of a match has its hero: the hall, the archive and
    /// everybody in it learn them, and the match is played.
    fn on_picked(&mut self, server: PlayerId, id: MatchId, picks: &[Pick]) {
        let Some(at) = self
            .matches
            .iter()
            .position(|m| m.id == id && m.server == Some(server) && m.stage == Stage::Picking)
        else {
            return;
        };
        let map = self.map_of(self.matches[at].mode);
        let m = &mut self.matches[at];
        for pick in picks {
            if let Some(seat) = m.roster.at_mut(pick.slot) {
                seat.hero = Some(pick.hero);
            }
        }
        m.stage = Stage::Playing;
        m.since = Instant::now();
        let summary = m.summary(map);
        self.archive.record(&summary, None);
        self.push_match_state(id);
        self.hall_dirty = true;
    }

    // ----- connections and sessions -----

    fn adopt(&mut self, stream: TcpStream) {
        let id = PlayerId(self.next_player);
        self.next_player += 1;
        self.conns.push(Connection::spawn::<ToHub, HubEvent>(
            id,
            stream,
            self.tx.clone(),
        ));
        self.sessions.push(Session::new(id));
    }

    fn outbox_of(&self, player: PlayerId) -> Option<Arc<Outbox>> {
        self.conns
            .iter()
            .find(|c| c.id == player)
            .map(|c| Arc::clone(&c.outbox))
    }

    fn send(&self, player: PlayerId, msg: &FromHub) {
        if let Some(conn) = self.conns.iter().find(|c| c.id == player) {
            conn.send(msg);
        }
    }

    fn refuse(&self, player: PlayerId, reason: HallReason) {
        self.send(player, &FromHub::Refused { reason });
    }

    fn session(&self, player: PlayerId) -> Option<&Session> {
        self.sessions.iter().find(|s| s.player == player)
    }

    fn session_mut(&mut self, player: PlayerId) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.player == player)
    }

    /// The account a connection signed in as.
    fn account_of(&self, player: PlayerId) -> Option<AccountId> {
        self.session(player).and_then(Session::account)
    }

    /// The display name of a connection.
    fn name_of(&self, player: PlayerId) -> String {
        match self.session(player).and_then(|s| s.who.clone()) {
            Some(Who::Account(id)) => self.accounts.login_of(id),
            Some(Who::Guest(name)) => name,
            None => String::new(),
        }
    }

    /// The connection an account is signed in on.
    fn player_of(&self, account: AccountId) -> Option<PlayerId> {
        self.sessions
            .iter()
            .find(|s| s.account() == Some(account))
            .map(|s| s.player)
    }

    /// Closes a connection and forgets it, its search included.
    fn drop_conn(&mut self, player: PlayerId) {
        self.leave_queue(player);
        if let Some(conn) = self.conns.iter().find(|c| c.id == player) {
            conn.close();
        }
        self.conns.retain(|c| c.id != player);
        self.sessions.retain(|s| s.player != player);
    }

    fn on_net(&mut self, event: NetEvent<ToHub>) {
        match event {
            NetEvent::Connected(stream) => self.adopt(stream),
            NetEvent::Disconnected(player) => {
                if self.session(player).is_some() {
                    self.leave_place(player);
                    self.drop_conn(player);
                }
            }
            NetEvent::Msg(player, msg) => {
                let Some(session) = self.session(player) else {
                    return;
                };
                if session.who.is_none() {
                    match msg {
                        ToHub::Hello { auth } => self.on_hello(player, auth),
                        _ => {
                            self.refuse(player, HallReason::NotSignedIn);
                            self.drop_conn(player);
                        }
                    }
                    return;
                }
                self.on_msg(player, msg);
            }
        }
    }

    // ----- signing in -----

    fn on_hello(&mut self, player: PlayerId, auth: Auth) {
        let now = Instant::now();
        let outcome: Result<(Who, Option<Token>), HallReason> = match auth {
            Auth::Register { login, password } => {
                let salt = salt();
                self.accounts
                    .register(&login, &password, salt, unix_now())
                    .map(|id| (Who::Account(id), None))
            }
            Auth::Login { login, password } => self
                .accounts
                .sign_in(&login, &password)
                .map(|id| (Who::Account(id), None)),
            Auth::Resume { token } => self
                .accounts
                .resume(token, now)
                .map(|id| (Who::Account(id), Some(token)))
                .ok_or(HallReason::UnknownToken),
            Auth::Guest { name } => {
                if guest_name_is_valid(&name) {
                    Ok((Who::Guest(name), None))
                } else {
                    Err(HallReason::BadName)
                }
            }
        };
        let (who, token) = match outcome {
            Ok(found) => found,
            Err(reason) => {
                self.refuse(player, reason);
                let failures = self.session_mut(player).map_or(0, |s| {
                    s.failures += 1;
                    s.failures
                });
                if failures >= SIGNIN_FAILURES {
                    self.drop_conn(player);
                }
                return;
            }
        };
        let account = match who {
            Who::Account(id) => Some(id),
            Who::Guest(_) => None,
        };
        // An account is signed in on one connection at a time; the newer
        // one wins.
        if let Some(id) = account
            && let Some(other) = self.player_of(id)
            && other != player
        {
            self.refuse(other, HallReason::SignedInElsewhere);
            self.leave_place(other);
            self.drop_conn(other);
        }
        let token = match (account, token) {
            (Some(id), None) => Some(self.accounts.issue(id, Token(fresh()), now)),
            (_, token) => token,
        };
        let info = account.map(|id| AccountInfo {
            id,
            login: self.accounts.login_of(id),
        });
        if let Some(session) = self.session_mut(player) {
            session.who = Some(who);
            session.token = token;
            session.place = Place::Hall;
        }
        self.send(
            player,
            &FromHub::Welcome {
                player_id: player,
                account: info,
                token,
                modes: self.opts.modes.iter().map(Mode::info).collect(),
            },
        );
        if !self.auto_place(player) {
            self.enter_hall(player);
        }
    }

    /// Puts a connection back into the gathering match its account holds a
    /// seat in, if any. Returns whether it did. A match being played is
    /// come back to with [`ToHub::JoinMatch`].
    fn auto_place(&mut self, player: PlayerId) -> bool {
        let Some(account) = self.account_of(player) else {
            return false;
        };
        let held = self
            .matches
            .iter()
            .find(|m| m.stage == Stage::Gathering && !m.roster.held_by(account).is_empty())
            .map(|m| m.id);
        match held {
            Some(id) => self
                .join_match(player, id, Role::Player { seats: 1 })
                .is_ok(),
            None => false,
        }
    }

    // ----- the hall -----

    fn hall_msg(&self) -> FromHub {
        let mut matches: Vec<_> = self
            .matches
            .iter()
            .map(|m| m.summary(self.map_of(m.mode)))
            .collect();
        matches.sort_by_key(|m| std::cmp::Reverse(m.id));
        let mut tournaments: Vec<_> = self
            .tournaments
            .iter()
            .rev()
            .filter(|t| t.info.stage != TournamentStage::Finished)
            .map(Tourney::summary)
            .collect();
        tournaments.extend(
            self.tournaments
                .iter()
                .rev()
                .filter(|t| t.info.stage == TournamentStage::Finished)
                .take(FINISHED_TOURNAMENTS_SHOWN)
                .map(Tourney::summary),
        );
        FromHub::Hall {
            matches,
            tournaments,
        }
    }

    fn enter_hall(&mut self, player: PlayerId) {
        if let Some(session) = self.session_mut(player) {
            session.place = Place::Hall;
        }
        let hall = self.hall_msg();
        self.send(player, &hall);
    }

    fn push_hall(&mut self) {
        let hall = self.hall_msg();
        for session in &self.sessions {
            if session.who.is_some() && session.place == Place::Hall {
                self.send(session.player, &hall);
            }
        }
        self.hall_dirty = false;
        self.hall_pushed = Some(Instant::now());
    }

    fn map_of(&self, mode: ModeId) -> bota_proto::MapId {
        self.mode(mode).map_or(bota_proto::MapId(0), |m| m.map)
    }

    fn mode(&self, id: ModeId) -> Option<&Mode> {
        self.opts.modes.iter().find(|m| m.id == id)
    }

    // ----- matches -----

    fn match_at(&self, id: MatchId) -> Option<usize> {
        self.matches.iter().position(|m| m.id == id)
    }

    fn push_match_state(&self, id: MatchId) {
        let Some(m) = self.matches.iter().find(|m| m.id == id) else {
            return;
        };
        for player in m.players() {
            self.send(
                player,
                &FromHub::MatchState {
                    state: m.state_for(player),
                },
            );
        }
    }

    /// Takes a connection out of whatever match it is in. The connection
    /// is left in no place; the caller decides where it goes.
    fn leave_place(&mut self, player: PlayerId) {
        let Some(id) = self.session(player).and_then(Session::in_match) else {
            return;
        };
        if let Some(session) = self.session_mut(player) {
            session.place = Place::Hall;
        }
        let Some(at) = self.match_at(id) else {
            return;
        };
        let account = self.account_of(player);
        let m = &mut self.matches[at];
        if !m.roster.sat_on_by(player).is_empty() {
            match m.stage {
                Stage::Gathering if !m.seats_fixed() => {
                    if let Some(account) = account {
                        m.roster.release(account);
                    }
                }
                _ => {
                    m.roster.away(player);
                }
            }
        }
        m.watchers.retain(|w| w.0 != player);
        m.since = Instant::now();
        // A host who leaves hands the match to whoever is seated, or takes
        // it down with them.
        if m.stage == Stage::Gathering && m.host.is_some() && m.host == account {
            m.host = m.roster.seats.iter().find_map(|s| s.account);
            if m.host.is_none() {
                self.cancel_match(id);
                return;
            }
        }
        self.push_match_state(id);
        self.hall_dirty = true;
    }

    /// Removes a match that never ran and sends everybody in it to the
    /// hall.
    fn cancel_match(&mut self, id: MatchId) {
        let Some(at) = self.match_at(id) else {
            return;
        };
        let players = self.matches[at].players();
        self.matches.remove(at);
        for player in players {
            self.enter_hall(player);
        }
        self.hall_dirty = true;
    }

    /// Forgets an ended match and sends everybody still in it to the hall.
    fn forget_match(&mut self, id: MatchId) {
        self.cancel_match(id);
    }

    fn create_match(
        &mut self,
        player: PlayerId,
        mode: ModeId,
        name: String,
        seed: Option<u64>,
        seats: u8,
    ) -> Result<MatchId, HallReason> {
        let account = self
            .account_of(player)
            .ok_or(HallReason::GuestsCannotPlay)?;
        let mode = self.mode(mode).cloned().ok_or(HallReason::NoSuchMode)?;
        if !title_is_valid(&name) {
            return Err(HallReason::BadName);
        }
        if !mode.admits(seats) {
            return Err(HallReason::TooManySeats);
        }
        self.leave_place(player);
        self.stop_searching(player);
        let id = self.archive.next_match_id();
        let seed = seed.unwrap_or_else(fresh_u64);
        let mut m = Match::new(id, name, &mode, Some(account), seed, unix_now());
        let login = self.accounts.login_of(account);
        let first = m.roster.free_on(Team::Radiant);
        for slot in first.into_iter().take(usize::from(seats)) {
            m.roster.seat(slot, account, &login, Some(player));
        }
        self.matches.push(m);
        if let Some(session) = self.session_mut(player) {
            session.place = Place::Match(id);
        }
        self.push_match_state(id);
        self.hall_dirty = true;
        Ok(id)
    }

    fn join_match(&mut self, player: PlayerId, id: MatchId, role: Role) -> Result<(), HallReason> {
        if self.match_at(id).is_none() {
            return Err(HallReason::NoSuchMatch);
        }
        let account = self.account_of(player);
        let name = self.name_of(player);
        // Being in it already is not a reason to leave it and come back.
        if self.session(player).and_then(Session::in_match) != Some(id) {
            self.leave_place(player);
        }
        let at = self.match_at(id).ok_or(HallReason::NoSuchMatch)?;
        let mode = self.mode(self.matches[at].mode).cloned();
        let m = &mut self.matches[at];
        let seats = match role {
            Role::Spectator => {
                if let Some(account) = account {
                    if m.stage == Stage::Gathering && !m.seats_fixed() {
                        // A seat holder who comes to watch gives the seats up.
                        m.roster.release(account);
                    } else {
                        m.roster.away(player);
                    }
                }
                Vec::new()
            }
            Role::Player { seats: wanted } => {
                let account = account.ok_or(HallReason::GuestsCannotPlay)?;
                let back = m.roster.sit_back(account, player);
                if !back.is_empty() {
                    back
                } else if m.stage != Stage::Gathering || m.seats_fixed() {
                    // A seat in a running match, and every seat of a
                    // tournament game or of a match from the queue, belongs
                    // to the account it was given to.
                    return Err(HallReason::NoSeat);
                } else {
                    if !mode.as_ref().is_some_and(|mode| mode.admits(wanted)) {
                        return Err(HallReason::TooManySeats);
                    }
                    let side = m.roster.side_for(wanted).ok_or(HallReason::MatchFull)?;
                    let login = self.accounts.login_of(account);
                    let slots: Vec<SlotId> = m
                        .roster
                        .free_on(side)
                        .into_iter()
                        .take(usize::from(wanted))
                        .collect();
                    for slot in &slots {
                        m.roster.seat(*slot, account, &login, Some(player));
                    }
                    slots
                }
            }
        };
        m.watchers.retain(|w| w.0 != player);
        if seats.is_empty() {
            m.watchers.push((player, name));
        }
        m.since = Instant::now();
        let started = m.started();
        if let Some(session) = self.session_mut(player) {
            session.place = Place::Match(id);
        }
        // A seat taken anywhere ends a search.
        if !seats.is_empty() {
            self.stop_searching(player);
        }
        self.push_match_state(id);
        if started {
            self.let_in(id, player, seats);
        }
        self.hall_dirty = true;
        self.maybe_start(id);
        Ok(())
    }

    /// Lets a connection into a match that has started, on its game server:
    /// with the pass its seats already have there, or a new one the game
    /// server is told of first. The connection is told where to go.
    fn let_in(&mut self, id: MatchId, player: PlayerId, seats: Vec<SlotId>) {
        let login = self.name_of(player);
        let Some(at) = self.match_at(id) else {
            return;
        };
        let Some(server) = self.matches[at].server else {
            return;
        };
        let Some(link) = self.links.iter().find(|link| link.conn.id == server) else {
            return;
        };
        let held = self.matches[at]
            .passes
            .iter()
            .find(|pass| !seats.is_empty() && pass.login == login && pass.seats == seats)
            .map(|pass| pass.ticket);
        let ticket = match held {
            Some(ticket) => ticket,
            None => {
                let pass = Pass {
                    ticket: Ticket(fresh()),
                    login,
                    seats,
                };
                link.conn.send(&Assign::Admit {
                    match_id: id,
                    pass: pass.clone(),
                });
                let ticket = pass.ticket;
                self.matches[at].passes.push(pass);
                ticket
            }
        };
        let addr = link
            .offer
            .as_ref()
            .map(|(addr, _)| addr.clone())
            .unwrap_or_default();
        self.send(
            player,
            &FromHub::Play {
                match_id: id,
                addr,
                ticket,
            },
        );
    }

    /// Gives up every seat the account holds in a match that has started,
    /// for good: its passes go, its game server is told, and a connection
    /// still in the match goes back to the hall.
    fn abandon(&mut self, player: PlayerId, id: MatchId) -> Result<(), HallReason> {
        let account = self
            .account_of(player)
            .ok_or(HallReason::GuestsCannotPlay)?;
        let at = self.match_at(id).ok_or(HallReason::NoSuchMatch)?;
        if !self.matches[at].started() {
            return Err(HallReason::NotPlaying);
        }
        let seats = self.matches[at].roster.abandon(account);
        if seats.is_empty() {
            return Err(HallReason::NoSeat);
        }
        let login = self.accounts.login_of(account);
        let m = &mut self.matches[at];
        m.passes
            .retain(|pass| !pass.seats.iter().any(|seat| seats.contains(seat)));
        if let Some(server) = m.server
            && let Some(link) = self.links.iter().find(|link| link.conn.id == server)
        {
            link.conn.send(&Assign::Abandon {
                match_id: id,
                seats,
                login,
            });
        }
        if self.session(player).and_then(Session::in_match) == Some(id) {
            self.leave_place(player);
            self.enter_hall(player);
        }
        self.push_match_state(id);
        self.hall_dirty = true;
        Ok(())
    }

    /// Starts a tournament game once every seat's holder is in it. A match
    /// from the queue starts as it is made, and one anybody created when its
    /// host says so.
    fn maybe_start(&mut self, id: MatchId) {
        let Some(at) = self.match_at(id) else {
            return;
        };
        let m = &self.matches[at];
        if m.stage == Stage::Gathering && m.tournament.is_some() && m.roster.all_present() {
            self.start_match(id);
        }
    }

    /// Starts the gathering match a connection is in and hosts, once every
    /// seat is held by an account that is in it.
    fn start_hosted(&mut self, player: PlayerId) -> Result<(), HallReason> {
        let account = self
            .account_of(player)
            .ok_or(HallReason::GuestsCannotPlay)?;
        let id = self
            .session(player)
            .and_then(Session::in_match)
            .ok_or(HallReason::NotInMatch)?;
        let at = self.match_at(id).ok_or(HallReason::NoSuchMatch)?;
        let m = &self.matches[at];
        if m.stage != Stage::Gathering {
            return Err(HallReason::NotGathering);
        }
        if m.host != Some(account) {
            return Err(HallReason::NotHost);
        }
        if !m.roster.all_present() {
            return Err(HallReason::NotAllHere);
        }
        self.start_match(id);
        Ok(())
    }

    // ----- matchmaking -----

    /// Puts a connection in the queue of a mode, in place of any search it
    /// had going, and makes whatever match the queue now can.
    fn find_match(&mut self, player: PlayerId, mode: ModeId, seats: u8) -> Result<(), HallReason> {
        let account = self
            .account_of(player)
            .ok_or(HallReason::GuestsCannotPlay)?;
        if !self.mode(mode).ok_or(HallReason::NoSuchMode)?.admits(seats) {
            return Err(HallReason::TooManySeats);
        }
        let busy = self.matches.iter().any(|m| {
            (m.started() || (m.stage == Stage::Gathering && m.seats_fixed()))
                && !m.roster.held_by(account).is_empty()
        });
        if busy {
            return Err(HallReason::MatchInProgress);
        }
        self.leave_queue(player);
        if self.session(player).and_then(Session::in_match).is_some() {
            self.leave_place(player);
            self.enter_hall(player);
        }
        let turn = self.next_turn;
        self.next_turn += 1;
        self.queue.push(Searcher {
            player,
            account,
            mode,
            seats,
            turn,
        });
        self.make_matches(mode);
        self.push_queue(mode);
        Ok(())
    }

    /// Ends a connection's search, if it has one going, and tells it so.
    fn stop_searching(&mut self, player: PlayerId) {
        let searching = self.queue.iter().any(|s| s.player == player)
            || self.found.iter().any(|found| found.has(player));
        if searching {
            self.leave_queue(player);
            self.push_search(player);
        }
    }

    /// Takes a connection out of the queue, and out of any match found for
    /// it, which falls through.
    fn leave_queue(&mut self, player: PlayerId) {
        if let Some(mode) = self
            .queue
            .iter()
            .find(|searcher| searcher.player == player)
            .map(|searcher| searcher.mode)
        {
            self.queue.retain(|searcher| searcher.player != player);
            self.push_queue(mode);
        }
        if let Some(at) = self.found.iter().position(|found| found.has(player)) {
            self.fall_through(at, |member| member.searcher.player == player);
        }
    }

    /// Finds every match the queue of a mode can fill and asks everybody it
    /// is for to accept it.
    fn make_matches(&mut self, mode: ModeId) {
        let Some(side) = self.mode(mode).map(|m| m.seats / 2) else {
            return;
        };
        while let Some(chosen) = fill_sides(&self.queue, mode, side) {
            let mut members: Vec<Member> = chosen
                .iter()
                .map(|&(at, side)| Member {
                    searcher: self.queue[at].clone(),
                    side,
                    accepted: false,
                })
                .collect();
            members.sort_by_key(|member| (member.side != Team::Radiant, member.searcher.turn));
            let taken: Vec<PlayerId> = members.iter().map(|m| m.searcher.player).collect();
            self.queue
                .retain(|searcher| !taken.contains(&searcher.player));
            self.found.push(Found {
                id: self.archive.next_match_id(),
                mode,
                members,
                deadline: Instant::now() + self.opts.accept_timeout,
            });
            for player in taken {
                self.push_search(player);
            }
        }
    }

    /// Accepts a found match for one connection, and makes the match once
    /// everybody has.
    fn accept(&mut self, player: PlayerId, id: MatchId) -> Result<(), HallReason> {
        let at = self
            .found
            .iter()
            .position(|found| found.id == id && found.has(player))
            .ok_or(HallReason::NoSuchMatch)?;
        for member in &mut self.found[at].members {
            if member.searcher.player == player {
                member.accepted = true;
            }
        }
        if self.found[at].members.iter().all(|member| member.accepted) {
            let found = self.found.remove(at);
            self.form_match(found);
        } else {
            for member in &self.found[at].members {
                self.push_search(member.searcher.player);
            }
        }
        Ok(())
    }

    /// Turns a found match down for one connection, which leaves the queue.
    fn decline(&mut self, player: PlayerId, id: MatchId) -> Result<(), HallReason> {
        let at = self
            .found
            .iter()
            .position(|found| found.id == id && found.has(player))
            .ok_or(HallReason::NoSuchMatch)?;
        self.fall_through(at, |member| member.searcher.player == player);
        Ok(())
    }

    /// Calls a found match off: the members `dropped` names leave the queue,
    /// the rest wait in it again at the places they had, and the queue makes
    /// whatever match it now can.
    fn fall_through(&mut self, at: usize, dropped: impl Fn(&Member) -> bool) {
        let found = self.found.remove(at);
        for member in found.members {
            let player = member.searcher.player;
            if dropped(&member) {
                self.notice(player, "you did not accept the match, and left the queue");
                self.push_search(player);
            } else {
                self.notice(
                    player,
                    "somebody did not accept the match; you are back in the queue",
                );
                self.queue.push(member.searcher);
            }
        }
        self.make_matches(found.mode);
        self.push_queue(found.mode);
    }

    /// Makes the match everybody accepted, each seat its account's from the
    /// start on the side the queue gave it, and starts it.
    fn form_match(&mut self, found: Found) {
        let Some(mode) = self.mode(found.mode).cloned() else {
            return;
        };
        let mut m = Match::new(
            found.id,
            mode.name.clone(),
            &mode,
            None,
            fresh_u64(),
            unix_now(),
        );
        m.matchmade = true;
        for member in &found.members {
            let login = self.accounts.login_of(member.searcher.account);
            let slots = m.roster.free_on(member.side);
            for slot in slots.into_iter().take(usize::from(member.searcher.seats)) {
                m.roster.seat(
                    slot,
                    member.searcher.account,
                    &login,
                    Some(member.searcher.player),
                );
            }
        }
        self.matches.push(m);
        for member in &found.members {
            let player = member.searcher.player;
            self.leave_place(player);
            if let Some(session) = self.session_mut(player) {
                session.place = Place::Match(found.id);
            }
            self.push_search(player);
        }
        self.hall_dirty = true;
        self.start_match(found.id);
        // One that waits for a game server is shown as it gathers.
        if self
            .match_at(found.id)
            .is_some_and(|at| self.matches[at].stage == Stage::Gathering)
        {
            self.push_match_state(found.id);
        }
    }

    /// Tells a connection where it stands in matchmaking.
    fn push_search(&self, player: PlayerId) {
        let search = if let Some(found) = self.found.iter().find(|found| found.has(player)) {
            Search::Found {
                id: found.id,
                mode: found.mode,
                accepted: u8::try_from(found.accepted()).unwrap_or(u8::MAX),
                players: u8::try_from(found.members.len()).unwrap_or(u8::MAX),
                yours: found
                    .members
                    .iter()
                    .any(|member| member.searcher.player == player && member.accepted),
                ms_left: u32::try_from(
                    found
                        .deadline
                        .saturating_duration_since(Instant::now())
                        .as_millis(),
                )
                .unwrap_or(u32::MAX),
            }
        } else if let Some(searcher) = self.queue.iter().find(|s| s.player == player) {
            let waiting = self
                .queue
                .iter()
                .filter(|other| other.mode == searcher.mode)
                .count();
            Search::Queued {
                mode: searcher.mode,
                waiting: u16::try_from(waiting).unwrap_or(u16::MAX),
            }
        } else {
            Search::Idle
        };
        self.send(player, &FromHub::Search { search });
    }

    /// Tells everybody waiting for a mode how many wait for it.
    fn push_queue(&self, mode: ModeId) {
        for searcher in self.queue.iter().filter(|s| s.mode == mode) {
            self.push_search(searcher.player);
        }
    }

    /// Calls off every found match whose time to accept has run out: who
    /// had not accepted leaves the queue.
    fn expire_found(&mut self, now: Instant) {
        while let Some(at) = self.found.iter().position(|found| found.deadline <= now) {
            self.fall_through(at, |member| !member.accepted);
        }
    }

    /// Hands a match to the game server with the most room, where its
    /// heroes are picked and it is played, and tells everybody in it where
    /// to go. With no room anywhere it waits, and says so once.
    fn start_match(&mut self, id: MatchId) {
        let Some(at) = self.match_at(id) else {
            return;
        };
        let Some(mode) = self.mode(self.matches[at].mode).cloned() else {
            return;
        };
        let roomiest = self
            .links
            .iter()
            .enumerate()
            .filter(|(_, link)| link.has_room())
            .min_by_key(|(_, link)| link.running.len())
            .map(|(link_at, _)| link_at);
        let Some(link_at) = roomiest else {
            if !self.matches[at].waiting_server {
                self.matches[at].waiting_server = true;
                for player in self.matches[at].players() {
                    self.notice(player, "the match waits for a game server with room");
                }
            }
            return;
        };
        let passes: Vec<(PlayerId, Pass)> = self.matches[at]
            .players()
            .into_iter()
            .map(|player| {
                let pass = Pass {
                    ticket: Ticket(fresh()),
                    login: self.name_of(player),
                    seats: self.matches[at].roster.sat_on_by(player),
                };
                (player, pass)
            })
            .collect();
        let replay = std::fs::File::create(self.archive.part_path(id)).ok();
        let m = &mut self.matches[at];
        m.stage = Stage::Picking;
        m.since = Instant::now();
        m.deadline = None;
        m.waiting_server = false;
        m.server = Some(self.links[link_at].conn.id);
        m.passes = passes.iter().map(|(_, pass)| pass.clone()).collect();
        m.replay = replay;
        let setup = MatchSetup {
            match_id: id,
            seed: m.seed,
            map: mode.map,
            seats: m.roster.setups(),
            pick_ms: u32::try_from(self.opts.pick_timeout.as_millis()).unwrap_or(u32::MAX),
            rules: bota_proto::Rules {
                cheats: mode.rules.cheats || self.opts.cheats,
                ..mode.rules
            },
            tick: mode.tick,
            tick_rate: mode.tick_rate,
            ack_timeout_ticks: mode.ack_timeout_ticks,
            abandon_after_ms: u32::try_from(self.opts.abandon_after.as_millis())
                .unwrap_or(u32::MAX),
            spectator_chat: mode.spectator_chat,
            passes: m.passes.clone(),
        };
        let summary = m.summary(mode.map);
        let link = &mut self.links[link_at];
        link.running.push(id);
        link.conn.send(&Assign::Start { setup });
        let addr = link
            .offer
            .as_ref()
            .map(|(addr, _)| addr.clone())
            .unwrap_or_default();
        self.archive.record(&summary, None);
        // Everybody hears that the match has started before where it plays,
        // as a connection joining a started match does.
        self.push_match_state(id);
        for (player, pass) in passes {
            self.send(
                player,
                &FromHub::Play {
                    match_id: id,
                    addr: addr.clone(),
                    ticket: pass.ticket,
                },
            );
        }
        self.hall_dirty = true;
    }

    /// A match ended on its game server, or with it: its replay takes its
    /// finished name, the archive and the tournament learn how it went, and
    /// a match waiting for room may now start.
    fn on_ended(&mut self, id: MatchId, winner: Option<Team>, stats: MatchStats, duration: u32) {
        let Some(at) = self.match_at(id) else {
            return;
        };
        if let Some(mut file) = self.matches[at].replay.take() {
            let _ = file.flush();
            drop(file);
            self.archive.finish_replay(id);
        }
        if let Some(server) = self.matches[at].server.take()
            && let Some(link) = self.links.iter_mut().find(|link| link.conn.id == server)
        {
            link.running.retain(|running| *running != id);
        }
        let map = self.map_of(self.matches[at].mode);
        let m = &mut self.matches[at];
        m.stage = if winner.is_some() {
            Stage::Over
        } else {
            Stage::Abandoned
        };
        m.winner = winner;
        m.stats = Some(stats);
        m.duration = duration;
        m.passes.clear();
        m.since = Instant::now();
        let tournament = m.tournament;
        let summary = m.summary(map);
        let stats = m.stats.clone();
        self.archive.record(&summary, stats.as_ref());
        self.push_match_state(id);
        self.hall_dirty = true;
        if let Some(t) = tournament {
            self.game_ended(t, id, winner);
        }
        self.start_waiting();
    }

    /// Where a connection may act on a gathering match it is seated in.
    fn seated_gathering(&self, player: PlayerId) -> Result<(usize, AccountId), HallReason> {
        let account = self
            .account_of(player)
            .ok_or(HallReason::GuestsCannotPlay)?;
        let id = self
            .session(player)
            .and_then(Session::in_match)
            .ok_or(HallReason::NotInMatch)?;
        let at = self.match_at(id).ok_or(HallReason::NoSuchMatch)?;
        let m = &self.matches[at];
        if m.stage != Stage::Gathering {
            return Err(HallReason::NotGathering);
        }
        if m.roster.sat_on_by(player).is_empty() {
            return Err(HallReason::NoSeat);
        }
        Ok((at, account))
    }

    fn on_msg(&mut self, player: PlayerId, msg: ToHub) {
        let outcome: Result<(), HallReason> = match msg {
            ToHub::Hello { .. } => Ok(()),
            ToHub::ChangePassword { old, new } => {
                let salt = salt();
                match self.account_of(player) {
                    None => Err(HallReason::GuestsCannotPlay),
                    Some(id) => self.accounts.change_password(id, &old, &new, salt),
                }
            }
            ToHub::Logout => {
                if let Some(token) = self.session(player).and_then(|s| s.token) {
                    self.accounts.revoke(token);
                }
                if let Some(session) = self.session_mut(player) {
                    session.token = None;
                }
                Ok(())
            }
            ToHub::ListMatches {
                filter,
                before,
                limit,
            } => {
                let (page, before) = self.page(player, &filter, before, limit);
                self.send(player, &FromHub::Matches { page, before });
                Ok(())
            }
            ToHub::CreateMatch {
                mode,
                name,
                seed,
                seats,
            } => self
                .create_match(player, mode, name, seed, seats)
                .map(|_| ()),
            ToHub::FindMatch { mode, seats } => self.find_match(player, mode, seats),
            ToHub::CancelSearch => {
                self.leave_queue(player);
                self.push_search(player);
                Ok(())
            }
            ToHub::Accept { id } => self.accept(player, id),
            ToHub::Decline { id } => self.decline(player, id),
            ToHub::JoinMatch { id, role } => self.join_match(player, id, role),
            ToHub::LeaveMatch => {
                self.leave_place(player);
                self.enter_hall(player);
                Ok(())
            }
            ToHub::StartMatch => self.start_hosted(player),
            ToHub::Abandon { id } => self.abandon(player, id),
            ToHub::TakeSeat { slot } => self.take_seat(player, slot),
            ToHub::DropSeat { slot } => self.drop_seat(player, slot),
            ToHub::Kick { account } => self.kick(player, account),
            ToHub::CreateTournament {
                name,
                mode,
                format,
                best_of,
            } => self.create_tournament(player, name, mode, format, best_of),
            ToHub::JoinTournament { id, team, seats } => {
                self.join_tournament(player, id, &team, seats)
            }
            ToHub::LeaveTournament => self.leave_tournament(player),
            ToHub::StartTournament { id } => self.start_tournament(player, id),
            ToHub::CancelTournament { id } => self.cancel_tournament(player, id),
            ToHub::ViewTournament { id } => self.view_tournament(player, id),
            ToHub::FetchReplay { match_id } => self.fetch_replay(player, match_id),
            ToHub::Chat { text, team } => {
                self.chat(player, &text, team);
                Ok(())
            }
        };
        if let Err(reason) = outcome {
            self.refuse(player, reason);
        }
    }

    // ----- chat -----

    /// Answers a chat line: a command when it starts with a slash, said where
    /// the connection is otherwise. A blank line is nothing.
    fn chat(&mut self, player: PlayerId, text: &str, team: bool) {
        if text.chars().count() > MAX_CHAT_CHARS {
            self.notice(player, "that line is too long");
            return;
        }
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        match text.strip_prefix('/') {
            Some(line) => self.run_command(player, line.trim()),
            None => self.say(player, text, team),
        }
    }

    /// Tells one connection a line from the server.
    fn notice(&self, player: PlayerId, text: impl Into<String>) {
        self.send(player, &FromHub::Notice { text: text.into() });
    }

    /// Who a connection speaks as where it is, or why it may not speak there.
    fn voice(&self, player: PlayerId) -> Result<Voice, &'static str> {
        let session = self.session(player).ok_or("sign in first")?;
        let account = session.account().ok_or("guests only watch")?;
        let login = self.accounts.login_of(account);
        let Some(id) = session.in_match() else {
            return Ok(Voice {
                login,
                side: None,
                at: None,
            });
        };
        let at = self.match_at(id).ok_or("that match is gone")?;
        let m = &self.matches[at];
        if m.started() {
            return Err("a match that has started talks on its game server");
        }
        let side = m.side_of_player(player);
        let spectators_write = self.mode(m.mode).is_some_and(|mode| mode.spectator_chat);
        if side.is_none() && !spectators_write {
            return Err("spectators do not write in this match");
        }
        Ok(Voice {
            login,
            side,
            at: Some(at),
        })
    }

    /// Says a line where the connection is, to one side alone when asked.
    fn say(&mut self, player: PlayerId, text: &str, team: bool) {
        let voice = match self.voice(player) {
            Ok(voice) => voice,
            Err(why) => return self.notice(player, why),
        };
        let side_only = voice.side.filter(|_| team);
        let said = FromHub::Chat {
            from: voice.login,
            side: voice.side,
            team: side_only.is_some(),
            text: text.to_string(),
        };
        self.spread(voice.at, side_only, &said);
    }

    /// Sends a message to everybody where a voice is heard: the hall, or a
    /// match. Told to one side, a match line reaches that side's seats and
    /// every spectator.
    fn spread(&self, at: Option<usize>, side_only: Option<Team>, msg: &FromHub) {
        let Some(at) = at else {
            for session in &self.sessions {
                if session.who.is_some() && session.place == Place::Hall {
                    self.send(session.player, msg);
                }
            }
            return;
        };
        let m = &self.matches[at];
        for listener in m.players() {
            let hears = match (side_only, m.side_of_player(listener)) {
                (Some(side), Some(theirs)) => side == theirs,
                _ => true,
            };
            if hears {
                self.send(listener, msg);
            }
        }
    }

    /// Runs a command typed after a slash.
    fn run_command(&mut self, player: PlayerId, line: &str) {
        let deed = match read_command(line) {
            Ok(deed) => deed,
            Err(why) => return self.notice(player, why),
        };
        match deed {
            Deed::Reply(lines) => {
                for text in lines {
                    self.notice(player, text);
                }
            }
            Deed::Roll { low, high } => {
                let voice = match self.voice(player) {
                    Ok(voice) => voice,
                    Err(why) => return self.notice(player, why),
                };
                let span = u64::from(high - low) + 1;
                let drawn = low + (fresh_u64() % span) as u32;
                let rolled = FromHub::Notice {
                    text: format!("{} rolls {drawn} ({low}-{high})", voice.login),
                };
                self.spread(voice.at, None, &rolled);
            }
            // A cheat is the business of a match that plays, on its game
            // server.
            Deed::Cheat { .. } => self.notice(player, "cheats work in a running match"),
        }
    }

    fn page(
        &self,
        player: PlayerId,
        filter: &MatchFilter,
        before: Option<MatchId>,
        limit: u16,
    ) -> (Vec<bota_proto::MatchSummary>, Option<MatchId>) {
        let login = filter.account.map(|id| self.accounts.login_of(id));
        let (mut page, next) = self.archive.page(filter, before, limit);
        if let Some(login) = login {
            page.retain(|s| s.seats.iter().any(|seat| seat.login == login));
        }
        let _ = player;
        (page, next)
    }

    fn take_seat(&mut self, player: PlayerId, slot: SlotId) -> Result<(), HallReason> {
        let account = self
            .account_of(player)
            .ok_or(HallReason::GuestsCannotPlay)?;
        let id = self
            .session(player)
            .and_then(Session::in_match)
            .ok_or(HallReason::NotInMatch)?;
        let at = self.match_at(id).ok_or(HallReason::NoSuchMatch)?;
        let login = self.accounts.login_of(account);
        let limit = self
            .mode(self.matches[at].mode)
            .map_or(1, |mode| mode.seats_per_player);
        let m = &mut self.matches[at];
        if m.stage != Stage::Gathering {
            return Err(HallReason::NotGathering);
        }
        if m.seats_fixed() {
            return Err(HallReason::SeatsFixed);
        }
        let taken = m.roster.at(slot).ok_or(HallReason::NoSeat)?;
        if taken.account == Some(account) {
            return Ok(()); // already this account's
        }
        if taken.account.is_some() {
            return Err(HallReason::SeatTaken);
        }
        if m.roster
            .side_of(account)
            .is_some_and(|team| team != taken.team)
        {
            return Err(HallReason::SeatsAcrossTeams);
        }
        if m.roster.held_by(account).len() >= usize::from(limit) {
            return Err(HallReason::TooManySeats);
        }
        m.roster.seat(slot, account, &login, Some(player));
        m.watchers.retain(|w| w.0 != player);
        m.since = Instant::now();
        self.push_match_state(id);
        self.hall_dirty = true;
        Ok(())
    }

    /// Gives up one seat of a gathering match, or all of them; a connection
    /// left with none watches.
    fn drop_seat(&mut self, player: PlayerId, slot: Option<SlotId>) -> Result<(), HallReason> {
        let (at, account) = self.seated_gathering(player)?;
        let name = self.name_of(player);
        let m = &mut self.matches[at];
        if m.seats_fixed() {
            return Err(HallReason::SeatsFixed);
        }
        match slot {
            None => {
                m.roster.release(account);
            }
            Some(slot) => {
                if m.roster.at(slot).and_then(|s| s.account) != Some(account) {
                    return Err(HallReason::NoSeat);
                }
                m.roster.release_slot(slot);
            }
        }
        if m.roster.held_by(account).is_empty() {
            m.watchers.retain(|w| w.0 != player);
            m.watchers.push((player, name));
        }
        let id = m.id;
        m.since = Instant::now();
        self.push_match_state(id);
        self.hall_dirty = true;
        Ok(())
    }

    fn kick(&mut self, player: PlayerId, target: AccountId) -> Result<(), HallReason> {
        let (at, account) = self.seated_gathering(player)?;
        let m = &mut self.matches[at];
        if m.host != Some(account) {
            return Err(HallReason::NotHost);
        }
        if m.seats_fixed() {
            return Err(HallReason::SeatsFixed);
        }
        let id = m.id;
        let thrown = m
            .roster
            .held_by(target)
            .first()
            .and_then(|slot| m.roster.at(*slot))
            .and_then(|seat| seat.player);
        m.roster.release(target);
        m.since = Instant::now();
        if let Some(thrown) = thrown {
            if let Some(session) = self.session_mut(thrown) {
                session.place = Place::Hall;
            }
            self.enter_hall(thrown);
        }
        self.push_match_state(id);
        self.hall_dirty = true;
        Ok(())
    }

    fn fetch_replay(&mut self, player: PlayerId, match_id: MatchId) -> Result<(), HallReason> {
        if !self.archive.has_replay(match_id) {
            return Err(HallReason::NoSuchReplay);
        }
        let outbox = self.outbox_of(player).ok_or(HallReason::NotSignedIn)?;
        self.fetcher.fetch(FetchJob {
            id: match_id,
            path: self.archive.replay_path(match_id),
            outbox,
        });
        Ok(())
    }

    // ----- tournaments -----

    fn tournament_at(&self, id: TournamentId) -> Option<usize> {
        self.tournaments.iter().position(|t| t.info.id == id)
    }

    /// The tournament an account is registered in and has not left, if
    /// any that is not finished.
    fn tournament_of(&self, account: AccountId) -> Option<usize> {
        self.tournaments
            .iter()
            .position(|t| t.info.stage != TournamentStage::Finished && t.team_of(account).is_some())
    }

    fn team_size(&self, mode: ModeId) -> usize {
        usize::from(self.mode(mode).map_or(2, |m| m.seats) / 2).max(1)
    }

    fn push_tournament(&self, at: usize) {
        let t = &self.tournaments[at];
        let msg = FromHub::TournamentState {
            state: t.info.clone(),
        };
        for session in &self.sessions {
            let member = session.account().is_some_and(|a| t.team_of(a).is_some());
            if session.who.is_some() && (member || session.viewing == Some(t.info.id)) {
                self.send(session.player, &msg);
            }
        }
    }

    fn save_tournament(&mut self, at: usize) {
        let info = self.tournaments[at].info.clone();
        self.archive.save_tournament(&info);
        self.push_tournament(at);
        self.hall_dirty = true;
    }

    fn create_tournament(
        &mut self,
        player: PlayerId,
        name: String,
        mode: ModeId,
        format: TournamentFormat,
        best_of: u8,
    ) -> Result<(), HallReason> {
        let account = self
            .account_of(player)
            .ok_or(HallReason::GuestsCannotPlay)?;
        if self.mode(mode).is_none() {
            return Err(HallReason::NoSuchMode);
        }
        if !title_is_valid(&name) {
            return Err(HallReason::BadName);
        }
        if !best_of_is_valid(best_of) {
            return Err(HallReason::BadSeries);
        }
        let id = self.archive.next_tournament_id();
        let info = TournamentInfo {
            id,
            name,
            mode,
            format,
            best_of,
            team_size: self.team_size(mode) as u8,
            organizer: self.accounts.login_of(account),
            stage: TournamentStage::Registration,
            teams: Vec::new(),
            rounds: Vec::new(),
            winner: None,
            created_at: unix_now(),
        };
        self.tournaments.push(Tourney::new(info, account));
        if let Some(session) = self.session_mut(player) {
            session.viewing = Some(id);
        }
        let at = self.tournaments.len() - 1;
        self.save_tournament(at);
        Ok(())
    }

    fn join_tournament(
        &mut self,
        player: PlayerId,
        id: TournamentId,
        team: &str,
        seats: u8,
    ) -> Result<(), HallReason> {
        let account = self
            .account_of(player)
            .ok_or(HallReason::GuestsCannotPlay)?;
        let at = self.tournament_at(id).ok_or(HallReason::NoSuchTournament)?;
        if self.tournament_of(account).is_some() {
            return Err(HallReason::InTournament);
        }
        if !title_is_valid(team) {
            return Err(HallReason::BadName);
        }
        let admitted = self
            .mode(self.tournaments[at].info.mode)
            .is_some_and(|mode| mode.admits(seats));
        if !admitted {
            return Err(HallReason::TooManySeats);
        }
        let login = self.accounts.login_of(account);
        self.tournaments[at].join(account, &login, team, seats)?;
        if let Some(session) = self.session_mut(player) {
            session.viewing = Some(id);
        }
        self.save_tournament(at);
        Ok(())
    }

    fn leave_tournament(&mut self, player: PlayerId) -> Result<(), HallReason> {
        let account = self
            .account_of(player)
            .ok_or(HallReason::GuestsCannotPlay)?;
        let at = self
            .tournament_of(account)
            .ok_or(HallReason::NotInTournament)?;
        let login = self.accounts.login_of(account);
        self.tournaments[at].leave(account, &login)?;
        self.save_tournament(at);
        Ok(())
    }

    fn start_tournament(&mut self, player: PlayerId, id: TournamentId) -> Result<(), HallReason> {
        let at = self.organised_by(player, id)?;
        self.tournaments[at].start()?;
        self.save_tournament(at);
        self.schedule(at);
        Ok(())
    }

    /// Takes a tournament down while it still registers, and tells everybody
    /// in it that it is gone.
    fn cancel_tournament(&mut self, player: PlayerId, id: TournamentId) -> Result<(), HallReason> {
        let at = self.organised_by(player, id)?;
        match self.tournaments[at].info.stage {
            TournamentStage::Registration => {}
            TournamentStage::Running => return Err(HallReason::TournamentRunning),
            TournamentStage::Finished => return Err(HallReason::TournamentStarted),
        }
        let members: Vec<AccountId> = self.tournaments[at]
            .members
            .iter()
            .map(|m| m.account)
            .collect();
        self.tournaments.remove(at);
        self.archive.forget_tournament(id);
        for session in &mut self.sessions {
            if session.viewing == Some(id) {
                session.viewing = None;
            }
        }
        for account in members {
            if let Some(player) = self.player_of(account) {
                self.refuse(player, HallReason::NoSuchTournament);
            }
        }
        self.hall_dirty = true;
        Ok(())
    }

    /// The tournament an account organises, by id.
    fn organised_by(&self, player: PlayerId, id: TournamentId) -> Result<usize, HallReason> {
        let account = self
            .account_of(player)
            .ok_or(HallReason::GuestsCannotPlay)?;
        let at = self.tournament_at(id).ok_or(HallReason::NoSuchTournament)?;
        if self.tournaments[at].organizer != account {
            return Err(HallReason::NotOrganizer);
        }
        Ok(at)
    }

    fn view_tournament(
        &mut self,
        player: PlayerId,
        id: Option<TournamentId>,
    ) -> Result<(), HallReason> {
        let at = match id {
            None => None,
            Some(id) => Some(self.tournament_at(id).ok_or(HallReason::NoSuchTournament)?),
        };
        if let Some(session) = self.session_mut(player) {
            session.viewing = id;
        }
        if let Some(at) = at {
            self.send(
                player,
                &FromHub::TournamentState {
                    state: self.tournaments[at].info.clone(),
                },
            );
        }
        Ok(())
    }

    /// Creates the next game of every pairing that can play, and brings
    /// its members in.
    fn schedule(&mut self, at: usize) {
        let mut changed = false;
        for (round, pair) in bracket::playable(&self.tournaments[at].info) {
            self.create_game(at, round, pair);
            changed = true;
        }
        if changed {
            self.save_tournament(at);
        }
    }

    fn create_game(&mut self, at: usize, round: usize, pair: usize) {
        let t = &self.tournaments[at];
        let entry = &t.info.rounds[round].pairs[pair];
        let (Some(a), Some(b)) = (entry.a, entry.b) else {
            return;
        };
        let game = entry.games.len() as u8;
        let Some(mode) = self.mode(t.info.mode).cloned() else {
            return;
        };
        let name = format!(
            "{}: {} vs {} ({})",
            t.info.name,
            t.info.teams[usize::from(a)].name,
            t.info.teams[usize::from(b)].name,
            game + 1
        );
        let id = self.archive.next_match_id();
        let seed = fresh_u64();
        let mut m = Match::new(id, name, &mode, None, seed, unix_now());
        m.tournament = Some(TournamentRef {
            id: t.info.id,
            round: round as u8,
            pair: pair as u8,
            game,
        });
        m.deadline = Some(Instant::now() + self.opts.ready_timeout);
        let side_a = side_of_a(game);
        for (team, side) in [(a, side_a), (b, other_side(side_a))] {
            for entrant in t.entrants_of(team) {
                let login = self.accounts.login_of(entrant.account);
                let free = m.roster.free_on(side);
                for slot in free.into_iter().take(usize::from(entrant.seats)) {
                    m.roster.seat(slot, entrant.account, &login, None);
                }
            }
        }
        let members: Vec<AccountId> = t
            .accounts_of(a)
            .into_iter()
            .chain(t.accounts_of(b))
            .collect();
        self.tournaments[at].info.rounds[round].pairs[pair]
            .games
            .push(GameInfo {
                match_id: Some(id),
                winner: None,
            });
        self.matches.push(m);
        self.hall_dirty = true;
        // Everybody the game is for is brought to it: from the hall, from a
        // cup screen, from a lobby they were waiting in, or from the game of
        // this series that has just ended. Only a match still being played
        // keeps somebody where they are.
        for account in members {
            if let Some(player) = self.player_of(account)
                && !self.mid_match(player)
            {
                let _ = self.join_match(player, id, Role::Player { seats: 1 });
            }
        }
        self.push_match_state(id);
    }

    /// Whether a connection is in a match that has started and not ended.
    fn mid_match(&self, player: PlayerId) -> bool {
        self.session(player)
            .and_then(Session::in_match)
            .and_then(|id| self.match_at(id))
            .is_some_and(|at| self.matches[at].started())
    }

    /// Records a finished game and moves the tournament along.
    fn game_ended(&mut self, t: TournamentRef, match_id: MatchId, winner: Option<Team>) {
        let Some(at) = self.tournament_at(t.id) else {
            return;
        };
        let Some((round, pair, game)) = self.tournaments[at].game_of(match_id) else {
            return;
        };
        match winner {
            None => self.tournaments[at].void(match_id),
            Some(side) => {
                let entry = &self.tournaments[at].info.rounds[round].pairs[pair];
                let team = if side == side_of_a(game as u8) {
                    entry.a
                } else {
                    entry.b
                };
                bracket::score(
                    &mut self.tournaments[at].info,
                    round,
                    pair,
                    GameInfo {
                        match_id: Some(match_id),
                        winner: team,
                    },
                );
            }
        }
        self.save_tournament(at);
        self.schedule(at);
    }

    /// The side that has not shown up by the deadline forfeits the game.
    fn walkover(&mut self, match_id: MatchId) {
        let Some(m_at) = self.match_at(match_id) else {
            return;
        };
        let Some(t) = self.matches[m_at].tournament else {
            return;
        };
        let m = &self.matches[m_at];
        if m.roster.all_present() {
            self.start_match(match_id);
            return;
        }
        let side_a = side_of_a(t.game);
        let present = |side: Team| {
            m.roster
                .seats
                .iter()
                .filter(|s| s.team == side && s.player.is_some())
                .count()
        };
        let a_more = present(side_a) >= present(other_side(side_a));
        self.cancel_match(match_id);
        let Some(at) = self.tournament_at(t.id) else {
            return;
        };
        let (round, pair) = (usize::from(t.round), usize::from(t.pair));
        let entry = &self.tournaments[at].info.rounds[round].pairs[pair];
        let winner = if a_more { entry.a } else { entry.b };
        bracket::score(
            &mut self.tournaments[at].info,
            round,
            pair,
            GameInfo {
                match_id: None,
                winner,
            },
        );
        self.save_tournament(at);
        self.schedule(at);
    }

    /// Picks stored tournaments up where a restart left them.
    fn resume_tournaments(&mut self) {
        for at in 0..self.tournaments.len() {
            if self.tournaments[at].info.stage != TournamentStage::Running {
                continue;
            }
            let open: Vec<MatchId> = self.tournaments[at]
                .info
                .rounds
                .iter()
                .flat_map(|r| &r.pairs)
                .flat_map(|p| &p.games)
                .filter(|g| g.winner.is_none())
                .filter_map(|g| g.match_id)
                .collect();
            for match_id in open {
                let recorded = self
                    .archive
                    .summary(match_id)
                    .filter(|s| s.stage == Stage::Over)
                    .and_then(|s| s.winner);
                let Some((round, pair, game)) = self.tournaments[at].game_of(match_id) else {
                    continue;
                };
                match recorded {
                    None => self.tournaments[at].void(match_id),
                    Some(side) => {
                        let entry = &self.tournaments[at].info.rounds[round].pairs[pair];
                        let team = if side == side_of_a(game as u8) {
                            entry.a
                        } else {
                            entry.b
                        };
                        bracket::score(
                            &mut self.tournaments[at].info,
                            round,
                            pair,
                            GameInfo {
                                match_id: Some(match_id),
                                winner: team,
                            },
                        );
                    }
                }
            }
            self.save_tournament(at);
            self.schedule(at);
        }
    }

    // ----- housekeeping -----

    fn housekeep(&mut self) {
        let now = Instant::now();
        if self.hall_dirty
            && self
                .hall_pushed
                .is_none_or(|last| now.duration_since(last) >= self.opts.hall_interval)
        {
            self.push_hall();
        }
        self.accounts.expire(now);
        let stale: Vec<MatchId> = self
            .matches
            .iter()
            .filter(|m| {
                !m.anyone_here()
                    && match m.stage {
                        Stage::Gathering => {
                            m.tournament.is_none() && m.since.elapsed() >= self.opts.empty_ttl
                        }
                        Stage::Picking | Stage::Playing => false,
                        Stage::Over | Stage::Abandoned => m.since.elapsed() >= self.opts.over_ttl,
                    }
            })
            .map(|m| m.id)
            .collect();
        for id in stale {
            self.forget_match(id);
        }
        let due: Vec<MatchId> = self
            .matches
            .iter()
            .filter(|m| {
                m.stage == Stage::Gathering
                    && !m.waiting_server
                    && m.deadline.is_some_and(|d| d <= now)
            })
            .map(|m| m.id)
            .collect();
        for id in due {
            self.walkover(id);
        }
        self.expire_found(now);
    }
}

/// Whether a match, tournament or team name is one to forty-eight
/// characters without control characters.
pub fn title_is_valid(name: &str) -> bool {
    (1..=TITLE_MAX).contains(&name.chars().count()) && !name.chars().any(char::is_control)
}

/// The side the first team of a pairing plays in a game: sides swap every
/// game of a series.
pub fn side_of_a(game: u8) -> Team {
    if game.is_multiple_of(2) {
        Team::Radiant
    } else {
        Team::Dire
    }
}

fn other_side(side: Team) -> Team {
    match side {
        Team::Radiant => Team::Dire,
        Team::Dire | Team::Neutral => Team::Radiant,
    }
}

/// The master key of a match's randomness, from its seed.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Sixteen bytes from the operating system's generator, to salt a password
/// with.
fn salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&fresh()[..16]);
    salt
}
