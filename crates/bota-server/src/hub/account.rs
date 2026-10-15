//! Accounts: the ledger on disk, password hashing and session tokens.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use bota_proto::{AccountId, HallReason, Token};

/// Fewest characters in a login.
pub const LOGIN_MIN: usize = 2;
/// Most characters in a login or a guest name.
pub const NAME_MAX: usize = 24;
/// Fewest characters in a password.
pub const PASSWORD_MIN: usize = 1;
/// Most characters in a password.
pub const PASSWORD_MAX: usize = 64;

/// How expensive the password hash is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashCost {
    /// Argon2id at its usual parameters.
    Full,
    /// The cheapest parameters the algorithm accepts.
    Light,
}

/// One registered account.
#[derive(Clone, Debug)]
pub struct Account {
    /// Which account.
    pub id: AccountId,
    /// Its name.
    pub login: String,
    /// The password hash, as a PHC string.
    hash: String,
}

/// A session key and whose it is.
struct Issued {
    token: Token,
    account: AccountId,
    expires: Instant,
}

/// Every account, the ledger it is appended to, and the tokens out.
pub struct Accounts {
    rows: Vec<Account>,
    ledger: Option<PathBuf>,
    cost: HashCost,
    tokens: Vec<Issued>,
    token_life: Duration,
}

impl Accounts {
    /// Reads the ledger, creating it when absent.
    pub fn open(ledger: &Path, cost: HashCost, token_life: Duration) -> std::io::Result<Accounts> {
        let mut accounts = Accounts {
            rows: Vec::new(),
            ledger: Some(ledger.to_path_buf()),
            cost,
            tokens: Vec::new(),
            token_life,
        };
        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(ledger)?;
        for line in BufReader::new(file).lines() {
            let line = line?;
            let fields: Vec<&str> = line.split('\t').collect();
            match fields.as_slice() {
                ["created", id, login, hash, ..] => {
                    if let Ok(id) = id.parse::<u64>() {
                        accounts.rows.push(Account {
                            id: AccountId(id),
                            login: (*login).to_string(),
                            hash: (*hash).to_string(),
                        });
                    }
                }
                ["password", id, hash, ..] => {
                    if let Ok(id) = id.parse::<u64>()
                        && let Some(row) = accounts.rows.iter_mut().find(|r| r.id.0 == id)
                    {
                        row.hash = (*hash).to_string();
                    }
                }
                _ => {}
            }
        }
        Ok(accounts)
    }

    /// Accounts kept in memory only.
    pub fn in_memory(cost: HashCost, token_life: Duration) -> Accounts {
        Accounts {
            rows: Vec::new(),
            ledger: None,
            cost,
            tokens: Vec::new(),
            token_life,
        }
    }

    /// The account with this id.
    pub fn get(&self, id: AccountId) -> Option<&Account> {
        self.rows.iter().find(|row| row.id == id)
    }

    /// The account with this login, case aside.
    pub fn by_login(&self, login: &str) -> Option<&Account> {
        self.rows
            .iter()
            .find(|row| row.login.eq_ignore_ascii_case(login))
    }

    /// The name of an account. Empty for one that does not exist.
    pub fn login_of(&self, id: AccountId) -> String {
        self.get(id)
            .map(|row| row.login.clone())
            .unwrap_or_default()
    }

    /// Makes an account. The salt is the caller's randomness.
    pub fn register(
        &mut self,
        login: &str,
        password: &str,
        salt: [u8; 16],
        created_at: u64,
    ) -> Result<AccountId, HallReason> {
        if !login_is_valid(login) {
            return Err(HallReason::BadName);
        }
        if !password_is_valid(password) {
            return Err(HallReason::BadPassword);
        }
        if self.by_login(login).is_some() {
            return Err(HallReason::LoginTaken);
        }
        let id = AccountId(self.rows.last().map_or(1, |row| row.id.0 + 1));
        let hash = hash_password(password, &salt, self.cost);
        self.append(&format!(
            "created\t{}\t{login}\t{hash}\t{created_at}\n",
            id.0
        ));
        self.rows.push(Account {
            id,
            login: login.to_string(),
            hash,
        });
        Ok(id)
    }

    /// Checks a login and password.
    pub fn sign_in(&self, login: &str, password: &str) -> Result<AccountId, HallReason> {
        let row = self.by_login(login).ok_or(HallReason::NoSuchLogin)?;
        if password_matches(password, &row.hash) {
            Ok(row.id)
        } else {
            Err(HallReason::WrongPassword)
        }
    }

    /// Replaces a password, after checking the old one.
    pub fn change_password(
        &mut self,
        id: AccountId,
        old: &str,
        new: &str,
        salt: [u8; 16],
    ) -> Result<(), HallReason> {
        let cost = self.cost;
        let row = self.rows.iter_mut().find(|row| row.id == id);
        let Some(row) = row else {
            return Err(HallReason::NoSuchLogin);
        };
        if !password_matches(old, &row.hash) {
            return Err(HallReason::WrongPassword);
        }
        if !password_is_valid(new) {
            return Err(HallReason::BadPassword);
        }
        row.hash = hash_password(new, &salt, cost);
        let line = format!("password\t{}\t{}\n", id.0, row.hash);
        self.append(&line);
        Ok(())
    }

    /// Hands out a token for an account. The token is the caller's
    /// randomness.
    pub fn issue(&mut self, account: AccountId, token: Token, now: Instant) -> Token {
        self.tokens.push(Issued {
            token,
            account,
            expires: now + self.token_life,
        });
        token
    }

    /// The account a token stands for, if it is out and not expired.
    pub fn resume(&self, token: Token, now: Instant) -> Option<AccountId> {
        self.tokens
            .iter()
            .find(|issued| issued.token == token && issued.expires > now)
            .map(|issued| issued.account)
    }

    /// Takes a token back.
    pub fn revoke(&mut self, token: Token) {
        self.tokens.retain(|issued| issued.token != token);
    }

    /// Drops every token past its time.
    pub fn expire(&mut self, now: Instant) {
        self.tokens.retain(|issued| issued.expires > now);
    }

    fn append(&self, line: &str) {
        if let Some(path) = &self.ledger
            && let Ok(mut file) = OpenOptions::new().append(true).create(true).open(path)
        {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }
}

/// Whether a login is two to twenty-four ASCII letters, digits, dots,
/// dashes or underscores.
pub fn login_is_valid(login: &str) -> bool {
    (LOGIN_MIN..=NAME_MAX).contains(&login.len())
        && login
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
}

/// Whether a guest name is one to twenty-four characters without control
/// characters.
pub fn guest_name_is_valid(name: &str) -> bool {
    (1..=NAME_MAX).contains(&name.chars().count()) && !name.chars().any(char::is_control)
}

/// Whether a password is one to sixty-four characters.
pub fn password_is_valid(password: &str) -> bool {
    (PASSWORD_MIN..=PASSWORD_MAX).contains(&password.chars().count())
}

fn hasher(cost: HashCost) -> Argon2<'static> {
    match cost {
        HashCost::Full => Argon2::default(),
        HashCost::Light => Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(
                Params::MIN_M_COST,
                Params::MIN_T_COST,
                Params::MIN_P_COST,
                None,
            )
            .expect("the minimum parameters are valid"),
        ),
    }
}

fn hash_password(password: &str, salt: &[u8; 16], cost: HashCost) -> String {
    let salt = SaltString::encode_b64(salt).expect("sixteen bytes encode");
    hasher(cost)
        .hash_password(password.as_bytes(), &salt)
        .expect("hashing a bounded password")
        .to_string()
}

fn password_matches(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}
