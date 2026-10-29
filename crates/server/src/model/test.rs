#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::string_slice)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::assert_is_empty)]

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
    let wrapped_vault_key = [7u8; 60];

    User::create(&pool, &identity_hash, verifier.clone(), &wrapped_vault_key)
        .await
        .context("failed to create user")?;

    let verified_user = User::verify(&pool, &identity_hash, verifier)
        .await
        .context("failed to verify user")?;
    // Test: verify user creation and lookup succeeds with matching verifier
    assert_eq!(
        verified_user,
        Some(crate::model::user::Verified {
            id: 1,
            wrapped_vault_key: wrapped_vault_key.to_vec(),
        })
    );

    Ok(())
}

#[tokio::test]
async fn user_create_duplicate_identity_rejected() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [2u8; 32];

    User::create(&pool, &identity_hash, "v1".to_string(), &[0u8; 60])
        .await
        .context("failed to create initial user")?;

    let duplicate_res =
        User::create(&pool, &identity_hash, "v2".to_string(), &[0u8; 60]).await;
    // Test: verify creating duplicate user identity is rejected
    assert!(duplicate_res.is_err());

    Ok(())
}

#[tokio::test]
async fn user_verify_mismatched_verifier_returns_none() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [3u8; 32];

    User::create(
        &pool,
        &identity_hash,
        "correct_verifier".to_string(),
        &[0u8; 60],
    )
    .await
    .context("failed to create user")?;

    let verified_id =
        User::verify(&pool, &identity_hash, "wrong_verifier".to_string())
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

    let verified_id = User::verify(&pool, &identity_hash, "any".to_string())
        .await
        .context("failed to execute user verification")?;
    // Test: verify non-existent user identity yields None
    assert!(verified_id.is_none());

    Ok(())
}

#[tokio::test]
async fn user_find_by_id_success() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [5u8; 32];
    let verifier = "verifier_5".to_string();
    let wrapped_vault_key = [8u8; 60];

    User::create(&pool, &identity_hash, verifier, &wrapped_vault_key).await?;

    let found = User::find_by_id(&pool, 1).await?;
    assert_eq!(
        found,
        Some(crate::model::user::Verified {
            id: 1,
            wrapped_vault_key: wrapped_vault_key.to_vec(),
        })
    );

    Ok(())
}

#[tokio::test]
async fn user_find_by_id_nonexistent_returns_none() -> Result<()> {
    let pool = setup_db().await?;
    let found = User::find_by_id(&pool, 9999).await?;
    assert!(found.is_none());

    Ok(())
}

#[tokio::test]
async fn user_rotate_recovery_success() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash1 = [20u8; 32];
    let verifier1 = "verifier_initial".to_string();
    let wrapped_key1 = [1u8; 60];

    User::create(&pool, &identity_hash1, verifier1.clone(), &wrapped_key1)
        .await?;

    let identity_hash2 = [21u8; 32];
    let verifier2 = "verifier_rotated".to_string();
    let wrapped_key2 = [2u8; 60];

    let rows_affected = User::rotate_recovery(
        &pool,
        1,
        &identity_hash2,
        &verifier2,
        &wrapped_key2,
    )
    .await?;
    assert_eq!(rows_affected, 1);

    // Old credentials no longer verify
    assert!(
        User::verify(&pool, &identity_hash1, verifier1)
            .await?
            .is_none()
    );

    // New credentials verify and return rotated wrapped_vault_key
    let verified = User::verify(&pool, &identity_hash2, verifier2)
        .await?
        .context("user must verify with new credentials")?;
    assert_eq!(verified.id, 1);
    assert_eq!(verified.wrapped_vault_key, wrapped_key2.to_vec());

    // find_by_id returns updated wrapped_vault_key
    let found = User::find_by_id(&pool, 1).await?.context("user exists")?;
    assert_eq!(found.wrapped_vault_key, wrapped_key2.to_vec());

    Ok(())
}

#[tokio::test]
async fn user_rotate_recovery_nonexistent_returns_zero() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [22u8; 32];
    let rows =
        User::rotate_recovery(&pool, 9999, &identity_hash, "v", &[0u8; 60])
            .await?;
    assert_eq!(rows, 0);

    Ok(())
}

#[tokio::test]
async fn user_rotate_recovery_duplicate_identity_rejected() -> Result<()> {
    let pool = setup_db().await?;
    let id1 = [23u8; 32];
    let id2 = [24u8; 32];

    User::create(&pool, &id1, "v1".to_string(), &[1u8; 60]).await?;
    User::create(&pool, &id2, "v2".to_string(), &[2u8; 60]).await?;

    // Attempting to rotate user 2 to id1 should fail with UniqueViolation
    let res = User::rotate_recovery(&pool, 2, &id1, "v_new", &[3u8; 60]).await;
    assert!(matches!(res, Err(crate::model::Error::UniqueViolation)));

    // User 2's credentials remain untouched
    let verified = User::verify(&pool, &id2, "v2".to_string()).await?;
    assert!(verified.is_some());

    Ok(())
}

#[tokio::test]
async fn session_create_wire_format_and_stateless_verify() -> Result<()> {
    let pool = setup_db().await?;
    let secret = Secret::random();

    // Create user first for foreign key
    let identity_hash = [10u8; 32];
    User::create(&pool, &identity_hash, "v".to_string(), &[0u8; 60])
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
    User::create(&pool, &identity_hash, "v".to_string(), &[0u8; 60])
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
    User::create(&pool, &identity_hash, "v".to_string(), &[0u8; 60])
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
    User::create(&pool, &identity_hash, "v".to_string(), &[0u8; 60])
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
async fn session_revoke_other_sessions_keeps_active() -> Result<()> {
    let pool = setup_db().await?;
    let secret = Secret::random();
    let identity_hash1 = [15u8; 32];
    let identity_hash2 = [16u8; 32];

    User::create(&pool, &identity_hash1, "v1".to_string(), &[0u8; 60]).await?;
    User::create(&pool, &identity_hash2, "v2".to_string(), &[0u8; 60]).await?;

    // Create 3 sessions for user 1
    let token1 = Session::create(&pool, 1, &secret).await?;
    let token2 = Session::create(&pool, 1, &secret).await?;
    let token3 = Session::create(&pool, 1, &secret).await?;

    // Create 1 session for user 2
    let token_u2 = Session::create(&pool, 2, &secret).await?;

    let hash1 = hash(token1.as_bytes());
    let hash2 = hash(token2.as_bytes());
    let hash3 = hash(token3.as_bytes());
    let hash_u2 = hash(token_u2.as_bytes());

    // Check find_other_token_hashes for user 1 keeping session 2
    let others =
        Session::find_other_token_hashes(&pool, 1, hash2.as_bytes()).await?;
    assert_eq!(others.len(), 2);
    let mut other_hashes: Vec<[u8; 32]> =
        others.into_iter().map(|th| th.0).collect();
    other_hashes.sort_unstable();
    let mut expected_hashes = vec![*hash1.as_bytes(), *hash3.as_bytes()];
    expected_hashes.sort_unstable();
    assert_eq!(other_hashes, expected_hashes);

    // Revoke other sessions keeping session 2
    let revoked =
        Session::revoke_other_sessions(&pool, 1, hash2.as_bytes()).await?;
    assert_eq!(revoked.len(), 2);

    // Session 2 is still valid
    let s2 = Session::verify(&pool, hash2.as_bytes())
        .await?
        .context("session 2 must exist")?;
    assert_eq!(s2.user_id, 1);

    // Sessions 1 and 3 are revoked
    assert!(Session::verify(&pool, hash1.as_bytes()).await?.is_none());
    assert!(Session::verify(&pool, hash3.as_bytes()).await?.is_none());

    // User 2's session is untouched
    let su2 = Session::verify(&pool, hash_u2.as_bytes())
        .await?
        .context("user 2 session must exist")?;
    assert_eq!(su2.user_id, 2);

    Ok(())
}

#[tokio::test]
async fn session_revoke_other_sessions_when_none_exist() -> Result<()> {
    let pool = setup_db().await?;
    let secret = Secret::random();
    let id_hash = [25u8; 32];
    User::create(&pool, &id_hash, "v".to_string(), &[0u8; 60]).await?;

    // User only has a single active session
    let token = Session::create(&pool, 1, &secret).await?;
    let token_hash = hash(token.as_bytes());

    let others =
        Session::find_other_token_hashes(&pool, 1, token_hash.as_bytes())
            .await?;
    assert!(others.is_empty());

    let revoked =
        Session::revoke_other_sessions(&pool, 1, token_hash.as_bytes()).await?;
    assert!(revoked.is_empty());

    // Original session is still valid
    assert!(
        Session::verify(&pool, token_hash.as_bytes())
            .await?
            .is_some()
    );

    Ok(())
}

#[tokio::test]
async fn session_lazy_revocation_on_expired_db_record() -> Result<()> {
    let pool = setup_db().await?;
    let identity_hash = [14u8; 32];
    User::create(&pool, &identity_hash, "v".to_string(), &[0u8; 60])
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

#[tokio::test]
async fn database_init_creates_db_and_migrates() -> Result<()> {
    let temp_dir = tmpdir::TmpDir::new("db-init-test")
        .await
        .context("failed to create temporary directory")?;
    let vault_path = temp_dir.to_path_buf();

    let config = kosh_core::config::Config {
        vault_path: vault_path.clone(),
        port: 23342,
        host: "0.0.0.0".to_string(),
        enable_tls: false,
        socket_path: "/tmp/kosh.sock".to_string(),
        passkey: kosh_core::config::Passkey::default(),
        mdns: kosh_core::config::Mdns::default(),
    };

    let pool = crate::model::database::init(&config)
        .await
        .context("database::init should succeed")?;

    // Verify metadata.db exists on disk
    let db_file = vault_path.join("metadata.db");
    assert!(db_file.exists());

    // Verify migrations were applied by querying users table
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .context("querying users table after migrations should succeed")?;
    assert_eq!(count, 0);

    // Verify idempotency when called again on the existing db
    let pool2 = crate::model::database::init(&config)
        .await
        .context("second database::init call should succeed")?;
    let (count2,): (i64,) = sqlx::query_as("SELECT count(*) FROM users")
        .fetch_one(&pool2)
        .await
        .context("querying users table from second pool should succeed")?;
    assert_eq!(count2, 0);

    Ok(())
}

#[tokio::test]
async fn database_init_fails_on_invalid_path() -> Result<()> {
    let config = kosh_core::config::Config {
        vault_path: std::path::PathBuf::from("/nonexistent_forbidden_dir_abc/nested"),
        port: 23342,
        host: "0.0.0.0".to_string(),
        enable_tls: false,
        socket_path: "/tmp/kosh.sock".to_string(),
        passkey: kosh_core::config::Passkey::default(),
        mdns: kosh_core::config::Mdns::default(),
    };

    let result = crate::model::database::init(&config).await;
    assert!(result.is_err(), "database::init must fail on invalid unwritable path");

    Ok(())
}


