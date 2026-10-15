//! The hub thread: every connection, where each one is, and what it may do.

use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bota_proto::{
    AccountId, AccountInfo, Auth, ClientMsg, GameInfo, HallReason, HeroId, MatchFilter, MatchId,
    ModeId, PlayerId, Role, ServerMsg, SlotId, Stage, Team, Token, TournamentFormat, TournamentId,
    TournamentInfo, TournamentRef, TournamentStage,
};
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{Rng as _, SeedableRng};

use crate::archive::{Archive, FetchJob, Fetcher};
use crate::game::MatchConfig;
use crate::hub::{
    Accounts, HubOpts, Match, Mode, Place, Session, Tourney, Who, best_of_is_valid, bracket,
    guest_name_is_valid,
};
use crate::net::{Connection, NetEvent, Outbox, accept_loop};
use crate::run::{MatchEnded, MatchEvent, MatchSetup, Participant, run_match};

/// Refused sign-ins on one connection before it is closed.
pub const SIGNIN_FAILURES: u8 = 5;
/// Most characters in a match or tournament name.
pub const TITLE_MAX: usize = 48;
/// Ended tournaments the hall keeps listing.
pub const FINISHED_TOURNAMENTS_SHOWN: usize = 10;

/// Everything that reaches the hub thread.
pub enum HubEvent {
    /// From a socket.
    Net(NetEvent),
    /// From a match thread.
    Ended(MatchEnded),
}

impl From<NetEvent> for HubEvent {
    fn from(event: NetEvent) -> HubEvent {
        HubEvent::Net(event)
    }
}

/// Runs the hub on an already-bound listener. Returns only when the event
/// channel is gone, which is never while the accept thread lives.
pub fn serve(listener: TcpListener, opts: HubOpts) -> std::io::Result<()> {
    let (tx, rx) = channel();
    let accept_tx = tx.clone();
    thread::spawn(move || accept_loop(listener, accept_tx));
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
    accounts: Accounts,
    archive: Archive,
    fetcher: Fetcher,
    matches: Vec<Match>,
    tournaments: Vec<Tourney>,
    next_player: u32,
    hall_dirty: bool,
    hall_pushed: Option<Instant>,
    rng: ChaCha8Rng,
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
            accounts,
            archive,
            fetcher: Fetcher::spawn(),
            matches: Vec::new(),
            tournaments,
            next_player: 1,
            hall_dirty: false,
            hall_pushed: None,
            rng: ChaCha8Rng::from_seed(os_seed()),
        };
        hub.resume_tournaments();
        Ok(hub)
    }

    /// Handles events until the channel closes.
    pub fn run(&mut self) {
        loop {
            match self.rx.recv_timeout(Duration::from_millis(250)) {
                Ok(HubEvent::Net(event)) => self.on_net(event),
                Ok(HubEvent::Ended(ended)) => self.on_ended(ended),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            self.housekeep();
        }
    }

    // ----- connections and sessions -----

    fn adopt(&mut self, stream: TcpStream) {
        let id = PlayerId(self.next_player);
        self.next_player += 1;
        self.conns
            .push(Connection::spawn(id, stream, self.tx.clone()));
        self.sessions.push(Session::new(id));
    }

    fn outbox_of(&self, player: PlayerId) -> Option<Arc<Outbox>> {
        self.conns
            .iter()
            .find(|c| c.id == player)
            .map(|c| Arc::clone(&c.outbox))
    }

    fn send(&self, player: PlayerId, msg: &ServerMsg) {
        if let Some(conn) = self.conns.iter().find(|c| c.id == player) {
            conn.send(msg);
        }
    }

    fn refuse(&self, player: PlayerId, reason: HallReason) {
        self.send(player, &ServerMsg::Refused { reason });
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

    /// Closes a connection and forgets it.
    fn drop_conn(&mut self, player: PlayerId) {
        if let Some(conn) = self.conns.iter().find(|c| c.id == player) {
            conn.close();
        }
        self.conns.retain(|c| c.id != player);
        self.sessions.retain(|s| s.player != player);
    }

    fn on_net(&mut self, event: NetEvent) {
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
                        ClientMsg::Hello { auth } => self.on_hello(player, auth),
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
                let salt = self.random_bytes();
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
            (Some(id), None) => {
                let fresh = Token(self.random_u128());
                Some(self.accounts.issue(id, fresh, now))
            }
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
            &ServerMsg::Welcome {
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

    /// Puts a connection back into the match its account holds a seat in,
    /// if any. Returns whether it did.
    fn auto_place(&mut self, player: PlayerId) -> bool {
        let Some(account) = self.account_of(player) else {
            return false;
        };
        let held = self
            .matches
            .iter()
            .find(|m| {
                matches!(m.stage, Stage::Gathering | Stage::Playing)
                    && m.roster.seat_of(account).is_some()
            })
            .map(|m| m.id);
        match held {
            Some(id) => self.join_match(player, id, Role::Player).is_ok(),
            None => false,
        }
    }

    // ----- the hall -----

    fn hall_msg(&self) -> ServerMsg {
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
        ServerMsg::Hall {
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

    fn random_bytes(&mut self) -> [u8; 16] {
        self.random_u128().to_le_bytes()
    }

    fn random_u128(&mut self) -> u128 {
        (u128::from(self.rng.next_u64()) << 64) | u128::from(self.rng.next_u64())
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
                &ServerMsg::MatchState {
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
        let mut told_run = false;
        if let Some(seat) = m.roster.seat_of_player(player).map(|s| s.slot) {
            match m.stage {
                Stage::Gathering if m.tournament.is_none() => {
                    if let Some(account) = account {
                        m.roster.release(account);
                    }
                }
                _ => {
                    m.roster.away(player);
                }
            }
            let _ = seat;
            if let Some(run) = &m.run {
                let _ = run.send(MatchEvent::Leave(player));
                told_run = true;
            }
        }
        if m.watchers.iter().any(|w| w.0 == player) {
            m.watchers.retain(|w| w.0 != player);
            if let Some(run) = &m.run
                && !told_run
            {
                let _ = run.send(MatchEvent::Leave(player));
            }
        }
        m.since = Instant::now();
        // A host who leaves hands the match to whoever is seated, or takes
        // it down with them.
        if m.stage == Stage::Gathering && m.tournament.is_none() && m.host == account {
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
    ) -> Result<MatchId, HallReason> {
        let account = self
            .account_of(player)
            .ok_or(HallReason::GuestsCannotPlay)?;
        let mode = self.mode(mode).cloned().ok_or(HallReason::NoSuchMode)?;
        if !title_is_valid(&name) {
            return Err(HallReason::BadName);
        }
        self.leave_place(player);
        let id = self.archive.next_match_id();
        let seed = seed.unwrap_or_else(|| self.rng.next_u64());
        let mut m = Match::new(id, name, &mode, Some(account), seed, unix_now());
        let login = self.accounts.login_of(account);
        m.roster.seat(SlotId(0), account, &login, Some(player));
        self.matches.push(m);
        if let Some(session) = self.session_mut(player) {
            session.place = Place::Match(id);
        }
        self.push_match_state(id);
        self.hall_dirty = true;
        Ok(id)
    }

    fn quick_play(&mut self, player: PlayerId, mode: ModeId) -> Result<(), HallReason> {
        let account = self
            .account_of(player)
            .ok_or(HallReason::GuestsCannotPlay)?;
        if self.mode(mode).is_none() {
            return Err(HallReason::NoSuchMode);
        }
        let open = self
            .matches
            .iter()
            .filter(|m| {
                m.stage == Stage::Gathering
                    && m.mode == mode
                    && m.tournament.is_none()
                    && (m.roster.seat_of(account).is_some()
                        || m.roster.seats.iter().any(|s| s.account.is_none()))
            })
            .map(|m| m.id)
            .next();
        match open {
            Some(id) => self.join_match(player, id, Role::Player),
            None => {
                let name = format!("{}'s game", self.accounts.login_of(account));
                self.create_match(player, mode, name, None).map(|_| ())
            }
        }
    }

    fn join_match(&mut self, player: PlayerId, id: MatchId, role: Role) -> Result<(), HallReason> {
        if self.match_at(id).is_none() {
            return Err(HallReason::NoSuchMatch);
        }
        let account = self.account_of(player);
        let name = self.name_of(player);
        let outbox = self.outbox_of(player);
        // Being in it already is not a reason to leave it and come back.
        if self.session(player).and_then(Session::in_match) != Some(id) {
            self.leave_place(player);
        }
        let at = self.match_at(id).ok_or(HallReason::NoSuchMatch)?;
        let m = &mut self.matches[at];
        let seat = match role {
            Role::Spectator => {
                if let Some(account) = account
                    && let Some(seat) = m.roster.seat_of(account).map(|s| s.slot)
                    && m.stage == Stage::Gathering
                    && m.tournament.is_none()
                {
                    // A seat holder who comes to watch gives the seat up.
                    let _ = seat;
                    m.roster.release(account);
                }
                if let Some(account) = account
                    && let Some(held) = m.roster.seat_of_mut(account)
                {
                    held.player = None;
                }
                None
            }
            Role::Player => {
                let account = account.ok_or(HallReason::GuestsCannotPlay)?;
                if let Some(held) = m.roster.seat_of_mut(account) {
                    held.player = Some(player);
                    Some(held.slot)
                } else if m.stage != Stage::Gathering || m.tournament.is_some() {
                    // A seat in a running match, and every seat of a
                    // tournament game, belongs to the account it was given
                    // to.
                    return Err(HallReason::NoSeat);
                } else {
                    let login = self.accounts.login_of(account);
                    let slot = m.roster.free_seat_mut().map(|s| s.slot);
                    let Some(slot) = slot else {
                        return Err(HallReason::MatchFull);
                    };
                    m.roster.seat(slot, account, &login, Some(player));
                    Some(slot)
                }
            }
        };
        m.watchers.retain(|w| w.0 != player);
        if seat.is_none() {
            m.watchers.push((player, name));
        }
        m.since = Instant::now();
        if let Some(run) = &m.run
            && let Some(outbox) = outbox
        {
            let _ = run.send(MatchEvent::Join(Participant {
                player,
                outbox,
                seat,
                eyes: None,
            }));
        }
        if let Some(session) = self.session_mut(player) {
            session.place = Place::Match(id);
        }
        self.push_match_state(id);
        self.hall_dirty = true;
        self.maybe_start(id);
        Ok(())
    }

    /// Starts a gathering match once every seat is held, present, picked
    /// and ready.
    fn maybe_start(&mut self, id: MatchId) {
        let Some(at) = self.match_at(id) else {
            return;
        };
        let m = &self.matches[at];
        if m.stage == Stage::Gathering && m.roster.all_ready() {
            self.start_match(id);
        }
    }

    fn start_match(&mut self, id: MatchId) {
        let Some(at) = self.match_at(id) else {
            return;
        };
        let Some(mode) = self.mode(self.matches[at].mode).cloned() else {
            return;
        };
        let cheats = mode.cheats || self.opts.cheats;
        let m = &mut self.matches[at];
        m.stage = Stage::Playing;
        m.since = Instant::now();
        m.deadline = None;
        let cfg = MatchConfig {
            match_id: m.id.0,
            master_key: key_from_seed(m.seed),
            picks: m.roster.picks(),
            map: mode.map,
            tick_rate: mode.tick_rate,
            mode: mode.tick,
            ack_timeout_ticks: mode.ack_timeout_ticks,
            cheats,
        };
        let (run_tx, run_rx) = channel();
        m.run = Some(run_tx);
        let summary = m.summary(mode.map);
        let players: Vec<(PlayerId, Option<SlotId>)> = m
            .roster
            .seats
            .iter()
            .filter_map(|s| s.player.map(|p| (p, Some(s.slot))))
            .chain(m.watchers.iter().map(|w| (w.0, None)))
            .collect();
        let participants = players
            .into_iter()
            .filter_map(|(player, seat)| {
                self.outbox_of(player).map(|outbox| Participant {
                    player,
                    outbox,
                    seat,
                    eyes: None,
                })
            })
            .collect();
        self.archive.record(&summary, None);
        let hub = self.tx.clone();
        let setup = MatchSetup {
            id,
            cfg,
            participants,
            events: run_rx,
            ended: Box::new(move |ended| {
                let _ = hub.send(HubEvent::Ended(ended));
            }),
            part: Some(self.archive.part_path(id)),
            finished: Some(self.archive.replay_path(id)),
            abandon_after: self.opts.abandon_after,
        };
        thread::spawn(move || run_match(setup));
        self.push_match_state(id);
        self.hall_dirty = true;
    }

    fn on_ended(&mut self, ended: MatchEnded) {
        let Some(at) = self.match_at(ended.id) else {
            return;
        };
        let map = self.map_of(self.matches[at].mode);
        let m = &mut self.matches[at];
        m.stage = if ended.winner.is_some() {
            Stage::Over
        } else {
            Stage::Abandoned
        };
        m.winner = ended.winner;
        m.stats = Some(ended.stats);
        m.duration = ended.duration;
        m.run = None;
        m.since = Instant::now();
        let tournament = m.tournament;
        let summary = m.summary(map);
        let stats = m.stats.clone();
        self.archive.record(&summary, stats.as_ref());
        self.push_match_state(ended.id);
        self.hall_dirty = true;
        if let Some(t) = tournament {
            self.game_ended(t, ended.id, ended.winner);
        }
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
        if m.roster.seat_of_player(player).is_none() {
            return Err(HallReason::NoSeat);
        }
        Ok((at, account))
    }

    fn on_msg(&mut self, player: PlayerId, msg: ClientMsg) {
        let outcome: Result<(), HallReason> = match msg {
            ClientMsg::Hello { .. } => Ok(()),
            ClientMsg::ChangePassword { old, new } => {
                let salt = self.random_bytes();
                match self.account_of(player) {
                    None => Err(HallReason::GuestsCannotPlay),
                    Some(id) => self.accounts.change_password(id, &old, &new, salt),
                }
            }
            ClientMsg::Logout => {
                if let Some(token) = self.session(player).and_then(|s| s.token) {
                    self.accounts.revoke(token);
                }
                if let Some(session) = self.session_mut(player) {
                    session.token = None;
                }
                Ok(())
            }
            ClientMsg::ListMatches {
                filter,
                before,
                limit,
            } => {
                let (page, before) = self.page(player, &filter, before, limit);
                self.send(player, &ServerMsg::Matches { page, before });
                Ok(())
            }
            ClientMsg::CreateMatch { mode, name, seed } => {
                self.create_match(player, mode, name, seed).map(|_| ())
            }
            ClientMsg::QuickPlay { mode } => self.quick_play(player, mode),
            ClientMsg::JoinMatch { id, role } => self.join_match(player, id, role),
            ClientMsg::LeaveMatch => {
                self.leave_place(player);
                self.enter_hall(player);
                Ok(())
            }
            ClientMsg::TakeSeat { slot } => self.take_seat(player, slot),
            ClientMsg::PickHero { hero } => self.pick_hero(player, hero),
            ClientMsg::SetReady(ready) => self.set_ready(player, ready),
            ClientMsg::Kick { account } => self.kick(player, account),
            ClientMsg::Order { seq, unit, order } => {
                self.forward(
                    player,
                    MatchEvent::Order {
                        player,
                        seq,
                        unit,
                        order,
                    },
                );
                Ok(())
            }
            ClientMsg::Ack { tick } => {
                self.forward(player, MatchEvent::Ack { player, tick });
                Ok(())
            }
            ClientMsg::ViewAs { seat } => {
                self.forward(player, MatchEvent::ViewAs { player, seat });
                Ok(())
            }
            ClientMsg::CreateTournament {
                name,
                mode,
                format,
                best_of,
            } => self.create_tournament(player, name, mode, format, best_of),
            ClientMsg::JoinTournament { id, team } => self.join_tournament(player, id, &team),
            ClientMsg::LeaveTournament => self.leave_tournament(player),
            ClientMsg::StartTournament { id } => self.start_tournament(player, id),
            ClientMsg::CancelTournament { id } => self.cancel_tournament(player, id),
            ClientMsg::ViewTournament { id } => self.view_tournament(player, id),
            ClientMsg::FetchReplay { match_id } => self.fetch_replay(player, match_id),
        };
        if let Err(reason) = outcome {
            self.refuse(player, reason);
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

    fn forward(&self, player: PlayerId, event: MatchEvent) {
        let Some(id) = self.session(player).and_then(Session::in_match) else {
            return;
        };
        if let Some(m) = self.matches.iter().find(|m| m.id == id)
            && let Some(run) = &m.run
        {
            let _ = run.send(event);
        }
    }

    fn take_seat(&mut self, player: PlayerId, slot: Option<SlotId>) -> Result<(), HallReason> {
        let account = self
            .account_of(player)
            .ok_or(HallReason::GuestsCannotPlay)?;
        let id = self
            .session(player)
            .and_then(Session::in_match)
            .ok_or(HallReason::NotInMatch)?;
        let at = self.match_at(id).ok_or(HallReason::NoSuchMatch)?;
        let name = self.name_of(player);
        let login = self.accounts.login_of(account);
        let m = &mut self.matches[at];
        if m.stage != Stage::Gathering {
            return Err(HallReason::NotGathering);
        }
        if m.tournament.is_some() {
            return Err(HallReason::SeatsFixed);
        }
        match slot {
            None => {
                m.roster.release(account);
                m.watchers.retain(|w| w.0 != player);
                m.watchers.push((player, name));
            }
            Some(slot) => {
                let taken = m
                    .roster
                    .seats
                    .iter()
                    .find(|s| s.slot == slot)
                    .ok_or(HallReason::NoSeat)?;
                if taken.account.is_some_and(|a| a != account) {
                    return Err(HallReason::SeatTaken);
                }
                m.roster.release(account);
                m.roster.seat(slot, account, &login, Some(player));
                m.watchers.retain(|w| w.0 != player);
            }
        }
        m.since = Instant::now();
        self.push_match_state(id);
        self.hall_dirty = true;
        Ok(())
    }

    fn pick_hero(&mut self, player: PlayerId, hero: HeroId) -> Result<(), HallReason> {
        let (at, account) = self.seated_gathering(player)?;
        if crate::game::hero_def(hero).is_none() {
            return Ok(()); // a pick of nobody leaves whatever was picked before
        }
        let id = self.matches[at].id;
        if let Some(seat) = self.matches[at].roster.seat_of_mut(account) {
            seat.hero = Some(hero);
        }
        self.push_match_state(id);
        Ok(())
    }

    fn set_ready(&mut self, player: PlayerId, ready: bool) -> Result<(), HallReason> {
        let (at, account) = self.seated_gathering(player)?;
        let id = self.matches[at].id;
        if let Some(seat) = self.matches[at].roster.seat_of_mut(account) {
            seat.ready = ready;
        }
        self.push_match_state(id);
        self.maybe_start(id);
        Ok(())
    }

    fn kick(&mut self, player: PlayerId, target: AccountId) -> Result<(), HallReason> {
        let (at, account) = self.seated_gathering(player)?;
        let m = &mut self.matches[at];
        if m.host != Some(account) {
            return Err(HallReason::NotHost);
        }
        if m.tournament.is_some() {
            return Err(HallReason::SeatsFixed);
        }
        let id = m.id;
        let thrown = m.roster.seat_of(target).and_then(|s| s.player);
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
        let msg = ServerMsg::TournamentState {
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
        let login = self.accounts.login_of(account);
        self.tournaments[at].join(account, &login, team)?;
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
        let members: Vec<AccountId> = self.tournaments[at].members.iter().map(|m| m.1).collect();
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
                &ServerMsg::TournamentState {
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
        let seed = self.rng.next_u64();
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
            let members = t.accounts_of(team);
            let free: Vec<SlotId> = m
                .roster
                .seats
                .iter()
                .filter(|s| s.team == side && s.account.is_none())
                .map(|s| s.slot)
                .collect();
            for (slot, account) in free.into_iter().zip(members) {
                let login = self.accounts.login_of(account);
                m.roster.seat(slot, account, &login, None);
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
                let _ = self.join_match(player, id, Role::Player);
            }
        }
        self.push_match_state(id);
    }

    /// Whether a connection is in a match that is still being played.
    fn mid_match(&self, player: PlayerId) -> bool {
        self.session(player)
            .and_then(Session::in_match)
            .and_then(|id| self.match_at(id))
            .is_some_and(|at| self.matches[at].stage == Stage::Playing)
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
                        Stage::Playing => false,
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
            .filter(|m| m.stage == Stage::Gathering && m.deadline.is_some_and(|d| d <= now))
            .map(|m| m.id)
            .collect();
        for id in due {
            self.walkover(id);
        }
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
fn key_from_seed(seed: u64) -> [u8; 32] {
    let mut key = [0u8; 32];
    for (i, b) in seed.to_le_bytes().iter().cycle().take(32).enumerate() {
        key[i] = *b;
    }
    key
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Thirty-two bytes of entropy from the process's randomly keyed hasher and
/// the clock.
fn os_seed() -> [u8; 32] {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut seed = [0u8; 32];
    for (i, chunk) in seed.chunks_mut(8).enumerate() {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(i as u64);
        hasher.write_u128(nanos);
        chunk.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    seed
}
