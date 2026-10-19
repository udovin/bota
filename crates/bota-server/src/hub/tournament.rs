//! A tournament as the hub holds it: its wire state and who is in it.

use bota_proto::{
    AccountId, HallReason, MatchId, MemberInfo, TeamInfo, TournamentFormat, TournamentInfo,
    TournamentStage, TournamentSummary,
};

use crate::hub::{Accounts, bracket};

/// Most games in a series.
pub const BEST_OF_MAX: u8 = 7;

/// One account registered in a tournament.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entrant {
    /// Its team, by index.
    pub team: u8,
    /// Which account.
    pub account: AccountId,
    /// How many of the team's seats it holds in every game.
    pub seats: u8,
}

/// One tournament.
pub struct Tourney {
    /// The state as the wire carries it.
    pub info: TournamentInfo,
    /// The account that created it.
    pub organizer: AccountId,
    /// Every member, in the order they joined.
    pub members: Vec<Entrant>,
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
                    .filter_map(|member| {
                        accounts.by_login(&member.login).map(|a| Entrant {
                            team: team as u8,
                            account: a.id,
                            seats: member.seats,
                        })
                    })
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
            .find(|m| m.account == account)
            .map(|m| m.team)
    }

    /// Accounts of one team, by index, in the order they joined.
    pub fn accounts_of(&self, team: u8) -> Vec<AccountId> {
        self.entrants_of(team)
            .into_iter()
            .map(|m| m.account)
            .collect()
    }

    /// Members of one team, by index, in the order they joined.
    pub fn entrants_of(&self, team: u8) -> Vec<Entrant> {
        self.members
            .iter()
            .filter(|m| m.team == team)
            .copied()
            .collect()
    }

    /// Seats a team's members hold between them, by its index on the wire.
    fn seats_of(&self, team: usize) -> usize {
        self.info.teams[team]
            .members
            .iter()
            .map(|m| usize::from(m.seats))
            .sum()
    }

    /// Registers an account in a team with a number of its seats, making the
    /// team when the name is new.
    pub fn join(
        &mut self,
        account: AccountId,
        login: &str,
        team: &str,
        seats: u8,
    ) -> Result<(), HallReason> {
        let team_size = usize::from(self.info.team_size);
        if self.info.stage != TournamentStage::Registration {
            return Err(HallReason::TournamentStarted);
        }
        if self.team_of(account).is_some() {
            return Err(HallReason::InTournament);
        }
        if seats == 0 || usize::from(seats) > team_size {
            return Err(HallReason::TooManySeats);
        }
        let at = match self
            .info
            .teams
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(team))
        {
            Some(at) => {
                if self.seats_of(at) + usize::from(seats) > team_size {
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
        self.info.teams[at].members.push(MemberInfo {
            login: login.to_string(),
            seats,
        });
        self.members.push(Entrant {
            team: at as u8,
            account,
            seats,
        });
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
        self.members.retain(|m| m.account != account);
        let entry = &mut self.info.teams[usize::from(team)];
        entry
            .members
            .retain(|m| !m.login.eq_ignore_ascii_case(login));
        if entry.members.is_empty() {
            self.info.teams.remove(usize::from(team));
            for member in &mut self.members {
                if member.team > team {
                    member.team -= 1;
                }
            }
        }
        Ok(())
    }

    /// Closes registration: teams whose members do not fill a side are
    /// dropped, the rest paired.
    pub fn start(&mut self) -> Result<(), HallReason> {
        let team_size = usize::from(self.info.team_size);
        if self.info.stage != TournamentStage::Registration {
            return Err(HallReason::TournamentStarted);
        }
        let full: Vec<usize> = (0..self.info.teams.len())
            .filter(|&i| self.seats_of(i) == team_size)
            .collect();
        if full.len() < 2 {
            return Err(HallReason::TooFewTeams);
        }
        let mut teams = Vec::new();
        let mut members = Vec::new();
        for (new_index, &old) in full.iter().enumerate() {
            teams.push(self.info.teams[old].clone());
            for member in &self.members {
                if usize::from(member.team) == old {
                    members.push(Entrant {
                        team: new_index as u8,
                        ..*member
                    });
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
