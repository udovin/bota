use bota_proto::{Auth, FromHub, HallReason, ToHub};

use super::fixtures::*;
use crate::hub::{login_is_valid, password_is_valid};

#[test]
fn register_login_and_resume_all_sign_in_and_the_wrong_ones_are_refused() {
    let (addr, _dir) = hub("auth");
    let (mut first, account, token) = sign_up(addr, "alice");
    assert_eq!(account.login, "alice");
    first.hall();

    let mut again = TestClient::connect(addr);
    again.send(&ToHub::Hello {
        auth: Auth::Register {
            login: "Alice".to_string(),
            password: "other".to_string(),
        },
    });
    assert_eq!(
        again.refusal(),
        HallReason::LoginTaken,
        "logins are one name in any case"
    );
    again.send(&ToHub::Hello {
        auth: Auth::Login {
            login: "alice".to_string(),
            password: "wrong".to_string(),
        },
    });
    assert_eq!(again.refusal(), HallReason::WrongPassword);
    again.send(&ToHub::Hello {
        auth: Auth::Login {
            login: "nobody".to_string(),
            password: "wrong".to_string(),
        },
    });
    assert_eq!(again.refusal(), HallReason::NoSuchLogin);
    again.send(&ToHub::Hello {
        auth: Auth::Resume {
            token: bota_proto::Token([1; 32]),
        },
    });
    assert_eq!(again.refusal(), HallReason::UnknownToken);
    again.send(&ToHub::Hello {
        auth: Auth::Resume { token },
    });
    let welcomed = again.recv_until(|msg| match msg {
        FromHub::Welcome { account, .. } => Some(account),
        _ => None,
    });
    assert_eq!(welcomed.map(|a| a.login), Some("alice".to_string()));
    assert_eq!(
        first.refusal(),
        HallReason::SignedInElsewhere,
        "the older connection is thrown out"
    );
    assert!(first.hung_up());

    let mut guest = TestClient::connect(addr);
    guest.send(&ToHub::Hello {
        auth: Auth::Guest {
            name: "eye".to_string(),
        },
    });
    let (account, token) = guest.recv_until(|msg| match msg {
        FromHub::Welcome { account, token, .. } => Some((account, token)),
        _ => None,
    });
    assert!(account.is_none() && token.is_none());
    guest.send(&ToHub::CreateMatch {
        mode: REALTIME,
        name: "no".to_string(),
        seed: None,
        seats: 1,
    });
    assert_eq!(guest.refusal(), HallReason::GuestsCannotPlay);
}

#[test]
fn five_refused_sign_ins_close_the_connection() {
    let (addr, _dir) = hub("failures");
    let mut client = TestClient::connect(addr);
    for _ in 0..5 {
        client.send(&ToHub::Hello {
            auth: Auth::Login {
                login: "nobody".to_string(),
                password: "x".to_string(),
            },
        });
        assert_eq!(client.refusal(), HallReason::NoSuchLogin);
    }
    assert!(client.hung_up());
}

#[test]
fn anything_before_hello_closes_the_connection() {
    let (addr, _dir) = hub("nohello");
    let mut client = TestClient::connect(addr);
    client.send(&ToHub::LeaveMatch);
    assert_eq!(client.refusal(), HallReason::NotSignedIn);
    assert!(client.hung_up());
}

#[test]
fn a_changed_password_is_the_one_that_signs_in_after_a_restart() {
    let dir = fresh_dir("restart");
    let addr = start_hub(test_opts(dir.clone()));
    let (mut alice, _, _) = sign_up(addr, "alice");
    alice.send(&ToHub::ChangePassword {
        old: "wrong".to_string(),
        new: "new-pw".to_string(),
    });
    assert_eq!(alice.refusal(), HallReason::WrongPassword);
    alice.send(&ToHub::ChangePassword {
        old: "alice-pw".to_string(),
        new: "new-pw".to_string(),
    });
    alice.send(&ToHub::Logout);
    // A second hub over the same directory reads the ledger back.
    let later = start_hub(test_opts(dir));
    let mut back = TestClient::connect(later);
    back.send(&ToHub::Hello {
        auth: Auth::Login {
            login: "alice".to_string(),
            password: "alice-pw".to_string(),
        },
    });
    assert_eq!(back.refusal(), HallReason::WrongPassword);
    back.send(&ToHub::Hello {
        auth: Auth::Login {
            login: "alice".to_string(),
            password: "new-pw".to_string(),
        },
    });
    let welcomed = back.recv_until(|msg| match msg {
        FromHub::Welcome { account, .. } => Some(account),
        _ => None,
    });
    assert_eq!(welcomed.map(|a| a.login), Some("alice".to_string()));
}

#[test]
fn names_and_passwords_have_bounds() {
    assert!(login_is_valid("ab"));
    assert!(login_is_valid("sylla.fan_2-x"));
    assert!(!login_is_valid("a"));
    assert!(!login_is_valid("with space"));
    assert!(!login_is_valid("ünïcode"));
    assert!(!login_is_valid(&"x".repeat(25)));
    assert!(password_is_valid("x"));
    assert!(!password_is_valid(""));
    assert!(!password_is_valid(&"x".repeat(65)));
}
