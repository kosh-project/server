#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]
#![allow(clippy::string_slice)]
#![allow(clippy::unnecessary_wraps)]

use super::{
    session::{Session, TokenHash},
    user::User,
};
use crate::auth::{Secret, TOKEN_LEN};
use anyhow::Result;
use blake3::{Hasher, hash, keyed_hash};
use sqlx::SqlitePool;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

async fn setup_db() -> Result<SqlitePool> {
    let pool = SqlitePool::connect("sqlite::memory:").await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

#[tokio::test]
async fn user_create_and_verify_success() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [1u8; 32];
    let verifier = "valid_argon2id_verifier".to_string();

    User::create(&pool, &identity_hash, verifier.clone()).await?;

    let verified_id =
        User::verify(&pool, identity_hash.to_vec(), verifier).await?;
    assert!(verified_id.is_some());
    assert_eq!(verified_id.unwrap(), 1);

    Ok(())
}

#[tokio::test]
async fn user_create_duplicate_identity_rejected() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [2u8; 32];

    User::create(&pool, &identity_hash, "v1".to_string()).await?;

    let duplicate_res =
        User::create(&pool, &identity_hash, "v2".to_string()).await;
    assert!(duplicate_res.is_err());

    Ok(())
}

#[tokio::test]
async fn user_verify_mismatched_verifier_returns_none() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [3u8; 32];

    User::create(&pool, &identity_hash, "correct_verifier".to_string()).await?;

    let verified_id = User::verify(
        &pool,
        identity_hash.to_vec(),
        "wrong_verifier".to_string(),
    )
    .await?;
    assert!(verified_id.is_none());

    Ok(())
}

#[tokio::test]
async fn user_verify_nonexistent_identity_returns_none() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [99u8; 32];

    let verified_id =
        User::verify(&pool, identity_hash.to_vec(), "any".to_string()).await?;
    assert!(verified_id.is_none());

    Ok(())
}

#[tokio::test]
async fn session_create_wire_format_and_stateless_verify() -> Result<()> {
    let pool = setup_db().await?;
    let secret = Secret::random();

    // Create user first for foreign key
    let identity_hash = [10u8; 32];
    User::create(&pool, &identity_hash, "v".to_string()).await?;

    let token = Session::create(&pool, 1, &secret).await?;
    assert_eq!(token.len(), TOKEN_LEN);

    // Verify UUID part (0..36)
    let session_uuid = &token[0..36];
    assert!(Uuid::parse_str(session_uuid).is_ok());

    // Verify delimiters
    assert_eq!(&token[36..37], ".");
    assert_eq!(&token[53..54], ".");

    // Verify timestamp part (37..53)
    let expires_hex = &token[37..53];
    assert_eq!(expires_hex.len(), 16);
    let expires_at = u64::from_str_radix(expires_hex, 16)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    assert!(expires_at > now);

    // Verify MAC part (54..118)
    let mac_hex = &token[54..];
    assert_eq!(mac_hex.len(), 64);
    assert!(mac_hex.chars().all(|c| c.is_ascii_hexdigit()));

    // Verify stateless validation passes
    let verify_res = Session::verify_stateless(&token, &secret);
    assert!(verify_res.is_ok());

    Ok(())
}

#[tokio::test]
async fn session_verify_stateless_tampered_mac() -> Result<()> {
    let pool = setup_db().await?;
    let secret = Secret::random();
    let identity_hash = [11u8; 32];
    User::create(&pool, &identity_hash, "v".to_string()).await?;

    let token = Session::create(&pool, 1, &secret).await?;

    // Tamper with last char of MAC
    let mut chars: Vec<char> = token.chars().collect();
    let last = chars.len() - 1;
    chars[last] = if chars[last] == 'a' { 'b' } else { 'a' };
    let tampered: String = chars.into_iter().collect();

    let res = Session::verify_stateless(&tampered, &secret);
    assert!(res.is_err());

    Ok(())
}

#[tokio::test]
async fn session_verify_stateless_tampered_payload() -> Result<()> {
    let pool = setup_db().await?;
    let secret = Secret::random();
    let identity_hash = [12u8; 32];
    User::create(&pool, &identity_hash, "v".to_string()).await?;

    let token = Session::create(&pool, 1, &secret).await?;

    // Tamper with first char of UUID
    let mut chars: Vec<char> = token.chars().collect();
    chars[0] = if chars[0] == '0' { '1' } else { '0' };
    let tampered: String = chars.into_iter().collect();

    let res = Session::verify_stateless(&tampered, &secret);
    assert!(res.is_err());

    Ok(())
}

#[test]
fn session_verify_stateless_expired() -> Result<()> {
    let secret = Secret::random();
    let session_id = Uuid::new_v4().to_string();
    let past_timestamp: i64 = 1_000_000; // Far in the past (1970)

    let payload = format!("{session_id}.{past_timestamp:016x}");
    let mac = keyed_hash(&secret.key(), payload.as_bytes());
    let token = format!("{payload}.{}", mac.to_hex());

    let res = Session::verify_stateless(&token, &secret);
    assert!(res.is_err());
    assert!(matches!(res, Err(crate::api::Error::Unauthorized("Token expired"))));

    Ok(())
}

#[test]
fn session_verify_stateless_invalid_length() {
    let secret = Secret::random();
    let short_token = "too_short_to_be_a_valid_token";
    let res = Session::verify_stateless(short_token, &secret);
    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::api::Error::Unauthorized("Invalid token format"))
    ));
}

#[tokio::test]
async fn session_stateful_verify_and_revoke() -> Result<()> {
    let pool = setup_db().await?;
    let secret = Secret::random();
    let identity_hash = [13u8; 32];
    User::create(&pool, &identity_hash, "v".to_string()).await?;

    let token = Session::create(&pool, 1, &secret).await?;
    let token_hash = hash(token.as_bytes());

    // Verify session in DB
    let session_opt = Session::verify(&pool, token_hash.as_bytes()).await?;
    assert!(session_opt.is_some());
    let session = session_opt.unwrap();
    assert_eq!(session.user_id, 1);

    // Revoke session
    Session::revoke(&pool, token_hash.as_bytes()).await?;

    // Verify it is gone
    let after_revoke = Session::verify(&pool, token_hash.as_bytes()).await?;
    assert!(after_revoke.is_none());

    Ok(())
}

#[tokio::test]
async fn session_lazy_revocation_on_expired_db_record() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [14u8; 32];
    User::create(&pool, &identity_hash, "v".to_string()).await?;

    let token_hash = hash(b"expired_session_token").as_bytes().to_vec();
    let past_created: i64 = 500;
    let past_expires: i64 = 1000;

    sqlx::query!(
        r#"
        INSERT INTO sessions (token_hash, user_id, created_at, expires_at)
        VALUES (?, ?, ?, ?)
        "#,
        token_hash,
        1,
        past_created,
        past_expires
    )
    .execute(&pool)
    .await?;

    // Verifying should trigger lazy deletion
    let session_opt = Session::verify(&pool, &token_hash).await?;
    assert!(session_opt.is_none());

    // Check DB row was deleted
    let row_count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) FROM sessions WHERE token_hash = ?"#,
        token_hash
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(row_count, 0);

    Ok(())
}

#[test]
fn token_hash_conversions() {
    let bytes = [9u8; 32];
    let th_from_ref = TokenHash::from(&bytes);
    assert_eq!(th_from_ref.0, bytes);
    assert_eq!(th_from_ref.as_ref(), &bytes[..]);

    let mut hasher = Hasher::new();
    hasher.update(b"sample");
    let blake_hash = hasher.finalize();

    let th_from_hasher: TokenHash = {
        let mut h = Hasher::new();
        h.update(b"sample");
        h.into()
    };
    let th_from_hash: TokenHash = blake_hash.into();

    assert_eq!(th_from_hasher, th_from_hash);
}
