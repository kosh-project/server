#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use super::{challenge::ChallengeQuery, *};
use crate::{
    api::Error::{BadRequest, Unauthorized},
    app::AppStateBuilder,
    auth::Secret,
};
use axum::extract::{ConnectInfo, Query, State};
use blake3::Hasher;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::{
    net::{IpAddr, SocketAddr},
    time::{SystemTime, UNIX_EPOCH},
};

fn forge_challenge(
    timestamp_secs: u64,
    ip: IpAddr,
    action: &str,
    secret: &Secret,
) -> String {
    let timestamp_hex = format!("{timestamp_secs:016x}");
    let mut hasher = Hasher::new_keyed(&secret.key());
    hasher.update(timestamp_hex.as_bytes());
    match ip {
        IpAddr::V4(addr) => hasher.update(&addr.octets()),
        IpAddr::V6(addr) => hasher.update(&addr.octets()),
    };
    hasher.update(action.as_bytes());
    let mac = hasher.finalize();

    format!("{timestamp_hex}{}{action}", mac.to_hex())
}

fn solve_challenge(challenge: &str, identity_hash: &str) -> String {
    for nonce in 0..u64::MAX {
        let candidate = format!("{identity_hash}{nonce:016x}{challenge}");
        let hash = Sha256::digest(candidate.as_bytes());
        if hash[0] == 0 && hash[1] == 0 {
            return candidate;
        }
    }
    panic!("Failed to find valid Proof of Work nonce");
}

#[tokio::test]
async fn challenge_generate_format() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("db connect");
    let secret = Secret::random();
    let state = AppStateBuilder::new()
        .vault_path("/tmp")
        .db(pool)
        .secret(secret)
        .build();

    let client_addr: SocketAddr = "127.0.0.1:45678".parse().unwrap();

    // Valid action: login
    let res = challenge::generate(
        ConnectInfo(client_addr),
        State(state.clone()),
        Query(ChallengeQuery {
            action: "login".to_string(),
        }),
    )
    .await
    .expect("challenge gen login");

    let login_challenge = res.0.challenge;
    assert_eq!(login_challenge.len(), 16 + 64 + 5);
    assert!(login_challenge.ends_with("login"));

    // Valid action: register
    let res = challenge::generate(
        ConnectInfo(client_addr),
        State(state.clone()),
        Query(ChallengeQuery {
            action: "register".to_string(),
        }),
    )
    .await
    .expect("challenge gen register");

    let reg_challenge = res.0.challenge;
    assert_eq!(reg_challenge.len(), 16 + 64 + 8);
    assert!(reg_challenge.ends_with("register"));

    // Invalid action
    let err = challenge::generate(
        ConnectInfo(client_addr),
        State(state),
        Query(ChallengeQuery {
            action: "invalid_action".to_string(),
        }),
    )
    .await;

    assert!(err.is_err());
    assert!(matches!(err, Err(BadRequest("Invalid action"))));
}

#[test]
fn hashcash_verify_stateless_success() {
    let secret = Secret::random();
    let ip: IpAddr = "127.0.0.1".parse().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let challenge = forge_challenge(now, ip, "login", &secret);
    let identity_hash =
        "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";

    let header = solve_challenge(&challenge, identity_hash);
    assert_eq!(header.len(), 165);

    let verified = HashCash::verify_stateless(&header, ip, &secret)
        .expect("stateless verification should succeed");
    assert_eq!(verified.identity_hash, identity_hash);
}

#[test]
fn hashcash_verify_stateless_insufficient_pow() {
    let secret = Secret::random();
    let ip: IpAddr = "127.0.0.1".parse().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let challenge = forge_challenge(now, ip, "login", &secret);
    let identity_hash =
        "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";

    // Fabricate header without solving PoW
    let arbitrary_nonce = "0000000000000000";
    let candidate = format!("{identity_hash}{arbitrary_nonce}{challenge}");

    // Force non-matching PoW
    let hash = Sha256::digest(candidate.as_bytes());
    if hash[0] != 0 || hash[1] != 0 {
        let res = HashCash::verify_stateless(&candidate, ip, &secret);
        assert!(res.is_err());
        assert!(matches!(res, Err(Unauthorized("Insufficient Proof of Work"))));
    }
}

#[test]
fn hashcash_verify_stateless_mismatched_ip() {
    let secret = Secret::random();
    let issued_ip: IpAddr = "192.168.1.100".parse().unwrap();
    let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let challenge = forge_challenge(now, issued_ip, "login", &secret);
    let identity_hash =
        "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";

    let header = solve_challenge(&challenge, identity_hash);

    let res = HashCash::verify_stateless(&header, client_ip, &secret);
    assert!(res.is_err());
    assert!(matches!(res, Err(Unauthorized("Forged or stolen challenge"))));
}

#[test]
fn hashcash_verify_stateless_expired_challenge() {
    let secret = Secret::random();
    let ip: IpAddr = "127.0.0.1".parse().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 20 seconds ago (> 15 second expiry)
    let expired_time = now - 20;
    let challenge = forge_challenge(expired_time, ip, "login", &secret);
    let identity_hash =
        "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";

    let header = solve_challenge(&challenge, identity_hash);

    let res = HashCash::verify_stateless(&header, ip, &secret);
    assert!(res.is_err());
    assert!(matches!(res, Err(Unauthorized("Challenge expired"))));
}

#[test]
fn hashcash_verify_stateless_future_timestamp() {
    let secret = Secret::random();
    let ip: IpAddr = "127.0.0.1".parse().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 10 seconds in future (> 5 second allowance)
    let future_time = now + 10;
    let challenge = forge_challenge(future_time, ip, "login", &secret);
    let identity_hash =
        "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";

    let header = solve_challenge(&challenge, identity_hash);

    let res = HashCash::verify_stateless(&header, ip, &secret);
    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(Unauthorized("Challenge timestamp is in the future"))
    ));
}

#[test]
fn hashcash_verify_stateless_invalid_header_length() {
    let secret = Secret::random();
    let ip: IpAddr = "127.0.0.1".parse().unwrap();

    let res = HashCash::verify_stateless("short_header", ip, &secret);
    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(BadRequest("Invalid X-Hashcash header length"))
    ));
}
