//! A tournament as the hub holds it: its wire state and who is in it.

use bota_proto::{
    AccountId, HallReason, MatchId, TeamInfo, TournamentFormat, TournamentInfo, TournamentStage,
    TournamentSummary,
};

use crate::hub::{Accounts, bracket};

/// Most games in a series.
pub const BEST_OF_MAX: u8 = 7;

/// One tournament.
pub struct Tourney {
    /// The state as the wire carries it.
    pub info: TournamentInfo,
    /// The account that created it.
    pub organizer: AccountId,
    /// Every member: which team, by index, and which account.
    pub members: Vec<(u8, AccountId)>,
}

impl Tourney {
    /// A tournament open for registration.
    pub fn new(info: TournamentInfo, organizer: AccountId) -> Tourney {
        Tourney {
            info,
            organizer,
            members: Vec::new(),
        }
    }

    /// A stored tournament, its members looked up by login. A login that
    /// no longer exists is left out.
    pub fn from_info(info: TournamentInfo, accounts: &Accounts) -> Tourney {
        let organizer = accounts
            .by_login(&info.organizer)
            .map_or(AccountId(0), |a| a.id);
        let members = info
            .teams
            .iter()
            .enumerate()
            .flat_map(|(team, t)| {
                t.members
                    .iter()
                    .filter_map(|login| accounts.by_login(login).map(|a| (team as u8, a.id)))
                    .collect::<Vec<_>>()
            })
            .collect();
        Tourney {
            info,
            organizer,
            members,
        }
    }

    /// The projection onto the hall.
    pub fn summary(&self) -> TournamentSummary {
        TournamentSummary {
            id: self.info.id,
            name: self.info.name.clone(),
            mode: self.info.mode,
            format: self.info.format,
            best_of: self.info.best_of,
            stage: self.info.stage,
            teams: self.info.teams.len() as u8,
            team_size: self.info.team_size,
        }
    }

    /// The team an account is in, by index.
    pub fn team_of(&self, account: AccountId) -> Option<u8> {
        self.members
            .iter()
            .find(|(_, a)| *a == account)
            .map(|(team, _)| *team)
    }

    /// Accounts of one team, by index.
    pub fn accounts_of(&self, team: u8) -> Vec<AccountId> {
        self.members
            .iter()
            .filter(|(t, _)| *t == team)
            .map(|(_, a)| *a)
            .collect()
    }

    /// Registers an account in a team, making the team when the name is
    /// new.
    pub fn join(&mut self, account: AccountId, login: &str, team: &str) -> Result<(), HallReason> {
        let team_size = usize::from(self.info.team_size);
        if self.info.stage != TournamentStage::Registration {
            return Err(HallReason::TournamentStarted);
        }
        if self.team_of(account).is_some() {
            return Err(HallReason::InTournament);
        }
        let at = match self
            .info
            .teams
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(team))
        {
            Some(at) => {
                if self.info.teams[at].members.len() >= team_size {
                    return Err(HallReason::TeamFull);
                }
                at
            }
            None => {
                if self.info.teams.len() >= usize::from(u8::MAX) {
                    return Err(HallReason::TeamFull);
                }
                self.info.teams.push(TeamInfo {
                    name: team.to_string(),
                    members: Vec::new(),
                });
                self.info.teams.len() - 1
            }
        };
        self.info.teams[at].members.push(login.to_string());
        self.members.push((at as u8, account));
        Ok(())
    }

    /// Withdraws an account. A team left empty is removed.
    pub fn leave(&mut self, account: AccountId, login: &str) -> Result<(), HallReason> {
        if self.info.stage != TournamentStage::Registration {
            return Err(HallReason::TournamentStarted);
        }
        let Some(team) = self.team_of(account) else {
            return Err(HallReason::NotInTournament);
        };
        self.members.retain(|(_, a)| *a != account);
        let entry = &mut self.info.teams[usize::from(team)];
        entry.members.retain(|m| !m.eq_ignore_ascii_case(login));
        if entry.members.is_empty() {
            self.info.teams.remove(usize::from(team));
            for (t, _) in &mut self.members {
                if *t > team {
                    *t -= 1;
                }
            }
        }
        Ok(())
    }

    /// Closes registration: teams short of members are dropped, the rest
    /// paired.
    pub fn start(&mut self) -> Result<(), HallReason> {
        let team_size = usize::from(self.info.team_size);
        if self.info.stage != TournamentStage::Registration {
            return Err(HallReason::TournamentStarted);
        }
        let full: Vec<usize> = (0..self.info.teams.len())
            .filter(|&i| self.info.teams[i].members.len() == team_size)
            .collect();
        if full.len() < 2 {
            return Err(HallReason::TooFewTeams);
        }
        let mut teams = Vec::new();
        let mut members = Vec::new();
        for (new_index, &old) in full.iter().enumerate() {
            teams.push(self.info.teams[old].clone());
            for (t, a) in &self.members {
                if usize::from(*t) == old {
                    members.push((new_index as u8, *a));
                }
            }
        }
        self.info.teams = teams;
        self.members = members;
        self.info.rounds = bracket::pair_up(self.info.format, self.info.teams.len() as u8);
        self.info.stage = TournamentStage::Running;
        bracket::settle(&mut self.info);
        Ok(())
    }

    /// Where a game of this tournament sits in the bracket, by its match.
    pub fn game_of(&self, match_id: MatchId) -> Option<(usize, usize, usize)> {
        for (r, round) in self.info.rounds.iter().enumerate() {
            for (p, pair) in round.pairs.iter().enumerate() {
                if let Some(g) = pair.games.iter().position(|g| g.match_id == Some(match_id)) {
                    return Some((r, p, g));
                }
            }
        }
        None
    }

    /// Forgets a game that produced no result, so the pairing plays it
    /// again.
    pub fn void(&mut self, match_id: MatchId) {
        for round in &mut self.info.rounds {
            for pair in &mut round.pairs {
                pair.games.retain(|g| g.match_id != Some(match_id));
            }
        }
    }
}

/// Whether a series length is one, three, five or seven.
pub fn best_of_is_valid(best_of: u8) -> bool {
    best_of % 2 == 1 && (1..=BEST_OF_MAX).contains(&best_of)
}

/// The format's name in the hall.
pub fn format_name(format: TournamentFormat) -> &'static str {
    match format {
        TournamentFormat::SingleElimination => "single elimination",
        TournamentFormat::RoundRobin => "round robin",
    }
}
