#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::string_slice)]
#![allow(clippy::unnecessary_wraps)]

use super::{
    session::{Session, TokenHash},
    user::User,
};
use crate::auth::{Secret, TOKEN_LEN};
use anyhow::{Context, Result};
use blake3::{Hasher, hash, keyed_hash};
use sqlx::SqlitePool;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

async fn setup_db() -> Result<SqlitePool> {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .context("db connect")?;
    sqlx::migrate!().run(&pool).await.context("migrate")?;
    Ok(pool)
}

#[tokio::test]
async fn user_create_and_verify_success() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [1u8; 32];
    let verifier = "valid_argon2id_verifier".to_string();

    User::create(&pool, &identity_hash, verifier.clone())
        .await
        .context("failed to create user")?;

    let verified_id = User::verify(&pool, identity_hash.to_vec(), verifier)
        .await
        .context("failed to verify user")?;
    // Test: verify user creation and lookup succeeds with matching verifier
    assert_eq!(verified_id, Some(1));

    Ok(())
}

#[tokio::test]
async fn user_create_duplicate_identity_rejected() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [2u8; 32];

    User::create(&pool, &identity_hash, "v1".to_string())
        .await
        .context("failed to create initial user")?;

    let duplicate_res =
        User::create(&pool, &identity_hash, "v2".to_string()).await;
    // Test: verify creating duplicate user identity is rejected
    assert!(duplicate_res.is_err());

    Ok(())
}

#[tokio::test]
async fn user_verify_mismatched_verifier_returns_none() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [3u8; 32];

    User::create(&pool, &identity_hash, "correct_verifier".to_string())
        .await
        .context("failed to create user")?;

    let verified_id = User::verify(
        &pool,
        identity_hash.to_vec(),
        "wrong_verifier".to_string(),
    )
    .await
    .context("failed to execute user verification")?;
    // Test: verify user verification fails when wrong verifier is supplied
    assert!(verified_id.is_none());

    Ok(())
}

#[tokio::test]
async fn user_verify_nonexistent_identity_returns_none() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [99u8; 32];

    let verified_id =
        User::verify(&pool, identity_hash.to_vec(), "any".to_string())
            .await
            .context("failed to execute user verification")?;
    // Test: verify non-existent user identity yields None
    assert!(verified_id.is_none());

    Ok(())
}

#[tokio::test]
async fn session_create_wire_format_and_stateless_verify() -> Result<()> {
    let pool = setup_db().await?;
    let secret = Secret::random();

    // Create user first for foreign key
    let identity_hash = [10u8; 32];
    User::create(&pool, &identity_hash, "v".to_string())
        .await
        .context("failed to create user")?;

    let token = Session::create(&pool, 1, &secret)
        .await
        .context("failed to create session token")?;
    // Test: verify token length conforms to TOKEN_LEN constant
    assert_eq!(token.len(), TOKEN_LEN);

    // Test: verify UUID component format (0..36)
    let session_uuid = &token[0..36];
    assert!(Uuid::parse_str(session_uuid).is_ok());

    // Test: verify dot delimiters separating token components
    assert_eq!(&token[36..37], ".");
    assert_eq!(&token[53..54], ".");

    // Test: verify timestamp component format and future expiration (37..53)
    let expires_hex = &token[37..53];
    assert_eq!(expires_hex.len(), 16);
    let expires_at = u64::from_str_radix(expires_hex, 16)
        .context("failed to parse expiration hex timestamp")?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time before epoch")?
        .as_secs();
    assert!(expires_at > now);

    // Test: verify MAC component format (54..118)
    let mac_hex = &token[54..];
    assert_eq!(mac_hex.len(), 64);
    assert!(mac_hex.chars().all(|c| c.is_ascii_hexdigit()));

    // Test: verify token passes stateless signature check
    let verify_res = Session::verify_stateless(&token, &secret);
    assert!(verify_res.is_ok());

    Ok(())
}

#[tokio::test]
async fn session_verify_stateless_tampered_mac() -> Result<()> {
    let pool = setup_db().await?;
    let secret = Secret::random();
    let identity_hash = [11u8; 32];
    User::create(&pool, &identity_hash, "v".to_string())
        .await
        .context("failed to create user")?;

    let token = Session::create(&pool, 1, &secret)
        .await
        .context("failed to create session token")?;

    // Tamper with last char of MAC
    let mut chars: Vec<char> = token.chars().collect();
    let last = chars.len() - 1;
    chars[last] = if chars[last] == 'a' { 'b' } else { 'a' };
    let tampered: String = chars.into_iter().collect();

    let res = Session::verify_stateless(&tampered, &secret);
    // Test: verify token with tampered signature bytes fails stateless verification
    assert!(res.is_err());

    Ok(())
}

#[tokio::test]
async fn session_verify_stateless_tampered_payload() -> Result<()> {
    let pool = setup_db().await?;
    let secret = Secret::random();
    let identity_hash = [12u8; 32];
    User::create(&pool, &identity_hash, "v".to_string())
        .await
        .context("failed to create user")?;

    let token = Session::create(&pool, 1, &secret)
        .await
        .context("failed to create session token")?;

    // Tamper with first char of UUID
    let mut chars: Vec<char> = token.chars().collect();
    chars[0] = if chars[0] == '0' { '1' } else { '0' };
    let tampered: String = chars.into_iter().collect();

    let res = Session::verify_stateless(&tampered, &secret);
    // Test: verify token with tampered payload bytes fails stateless verification
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
    // Test: verify expired token returns Unauthorized error
    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::api::Error::Unauthorized("Token expired"))
    ));

    Ok(())
}

#[test]
fn session_verify_stateless_invalid_length() -> Result<()> {
    let secret = Secret::random();
    let short_token = "too_short_to_be_a_valid_token";
    let res = Session::verify_stateless(short_token, &secret);
    // Test: verify token with invalid length returns Unauthorized error
    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::api::Error::Unauthorized("Invalid token format"))
    ));

    Ok(())
}

#[tokio::test]
async fn session_stateful_verify_and_revoke() -> Result<()> {
    let pool = setup_db().await?;
    let secret = Secret::random();
    let identity_hash = [13u8; 32];
    User::create(&pool, &identity_hash, "v".to_string())
        .await
        .context("failed to create user")?;

    let token = Session::create(&pool, 1, &secret)
        .await
        .context("failed to create session token")?;
    let token_hash = hash(token.as_bytes());

    // Verify session in DB
    let session_opt = Session::verify(&pool, token_hash.as_bytes())
        .await
        .context("failed to verify session")?;
    // Test: verify active session is found in database
    assert!(session_opt.is_some());
    let session = session_opt.context("session should be present")?;
    assert_eq!(session.user_id, 1);

    // Revoke session
    Session::revoke(&pool, token_hash.as_bytes())
        .await
        .context("failed to revoke session")?;

    // Verify it is gone
    let after_revoke = Session::verify(&pool, token_hash.as_bytes())
        .await
        .context("failed to re-verify revoked session")?;
    // Test: verify revoked session is no longer retrievable
    assert!(after_revoke.is_none());

    Ok(())
}

#[tokio::test]
async fn session_lazy_revocation_on_expired_db_record() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [14u8; 32];
    User::create(&pool, &identity_hash, "v".to_string())
        .await
        .context("failed to create user")?;

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
    .await
    .context("failed to insert expired test session")?;

    // Verifying should trigger lazy deletion
    let session_opt = Session::verify(&pool, &token_hash)
        .await
        .context("failed to verify expired session")?;
    // Test: verify verifying expired session triggers lazy deletion and returns None
    assert!(session_opt.is_none());

    // Check DB row was deleted
    let row_count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) FROM sessions WHERE token_hash = ?"#,
        token_hash
    )
    .fetch_one(&pool)
    .await
    .context("failed to count sessions after lazy deletion")?;
    // Test: verify expired session row is permanently deleted from database
    assert_eq!(row_count, 0);

    Ok(())
}

#[test]
fn token_hash_conversions() -> Result<()> {
    let bytes = [9u8; 32];
    let th_from_ref = TokenHash::from(&bytes);
    // Test: verify TokenHash conversion from reference preserves byte array
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

    // Test: verify TokenHash from Hasher and Hash instances are identical
    assert_eq!(th_from_hasher, th_from_hash);

    Ok(())
}
