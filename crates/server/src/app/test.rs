#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]

use super::*;
use crate::auth::Secret;
use anyhow::Context;
use sqlx::SqlitePool;
use std::path::Path;
use uuid::Uuid;
use webauthn_rs::WebauthnBuilder;

#[tokio::test]
async fn app_state_builder_builds_valid_state() -> anyhow::Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .context("db connect")?;
    let secret = Secret::random();
    let vault_path = "/tmp/test_vault";

    let state = AppStateBuilder::new()
        .vault_path(vault_path)
        .db(pool)
        .secret(secret)
        .build();

    // Test: verify state builder preserves configured vault path and secret key
    assert_eq!(state.vault_path(), Path::new(vault_path));
    assert_eq!(state.secret.key(), secret.key());

    Ok(())
}

#[tokio::test]
#[should_panic(expected = "FATAL: vault_path is required!")]
async fn app_state_builder_panics_on_missing_vault_path() {
    let Ok(pool) = SqlitePool::connect("sqlite::memory:").await else {
        return;
    };
    let secret = Secret::random();

    // Test: verify state builder panics when vault_path is not configured
    let _ = AppStateBuilder::new().db(pool).secret(secret).build();
}

#[test]
#[should_panic(expected = "FATAL: database pool is required!")]
fn app_state_builder_panics_on_missing_db() {
    let secret = Secret::random();

    // Test: verify state builder panics when database pool is not configured
    let _ = AppStateBuilder::new()
        .vault_path("/tmp")
        .secret(secret)
        .build();
}

#[tokio::test]
#[should_panic(expected = "FATAL: no secret provided??")]
async fn app_state_builder_panics_on_missing_secret() {
    let Ok(pool) = SqlitePool::connect("sqlite::memory:").await else {
        return;
    };

    // Test: verify state builder panics when secret key is not configured
    let _ = AppStateBuilder::new().vault_path("/tmp").db(pool).build();
}

#[tokio::test]
async fn app_state_builder_with_custom_webauthn() -> anyhow::Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .context("db connect")?;
    let secret = Secret::random();
    let origin = url::Url::parse("https://auth.example.com")?;
    let webauthn = WebauthnBuilder::new("auth.example.com", &origin)?
        .rp_name("Custom RP")
        .build()?;

    let state = AppStateBuilder::new()
        .vault_path("/tmp/custom_vault")
        .db(pool)
        .secret(secret)
        .webauthn(webauthn)
        .build();

    let (_challenge, auth_state) =
        state.webauthn.start_discoverable_authentication()?;
    let id = state.passkey.store_auth(auth_state).await;
    let taken = state.passkey.take_auth(&id).await;
    assert!(taken.is_ok());

    Ok(())
}

#[tokio::test]
async fn passkey_auth_cache_lifecycle() -> anyhow::Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .context("db connect")?;
    let secret = Secret::random();
    let state = AppStateBuilder::new()
        .vault_path("/tmp/test_vault")
        .db(pool)
        .secret(secret)
        .build();

    let (_challenge, auth_state) =
        state.webauthn.start_discoverable_authentication()?;

    // 1. Store auth challenge
    let id = state.passkey.store_auth(auth_state).await;

    // 2. Take auth challenge succeeds first time
    let taken = state.passkey.take_auth(&id).await;
    assert!(taken.is_ok());

    // 3. Second take fails because it was consumed
    let second_take = state.passkey.take_auth(&id).await;
    assert!(matches!(
        second_take,
        Err(crate::api::Error::BadRequest(
            "Challenge expired or invalid"
        ))
    ));

    // 4. Non-existent UUID fails
    let random_id = Uuid::new_v4();
    let nonexistent = state.passkey.take_auth(&random_id).await;
    assert!(matches!(
        nonexistent,
        Err(crate::api::Error::BadRequest(
            "Challenge expired or invalid"
        ))
    ));

    Ok(())
}

#[tokio::test]
async fn passkey_registration_cache_lifecycle() -> anyhow::Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .context("db connect")?;
    let secret = Secret::random();
    let state = AppStateBuilder::new()
        .vault_path("/tmp/test_vault")
        .db(pool)
        .secret(secret)
        .build();

    let user_id = 42;
    let (_challenge, reg_state) = state.webauthn.start_passkey_registration(
        Uuid::new_v4(),
        "user_42",
        "User 42",
        None,
    )?;

    // 1. Store registration challenge
    let id = state.passkey.store_registration(user_id, reg_state).await;

    // 2. Mismatched user_id returns Unauthorized
    let wrong_user = 99;
    let mismatched = state.passkey.take_registration(&id, wrong_user).await;
    assert!(matches!(
        mismatched,
        Err(crate::api::Error::Unauthorized(
            "Mismatched user for challenge"
        ))
    ));

    // 3. Since it was removed from cache, subsequent call is now expired/invalid
    let retry = state.passkey.take_registration(&id, user_id).await;
    assert!(matches!(
        retry,
        Err(crate::api::Error::BadRequest(
            "Challenge expired or invalid"
        ))
    ));

    // 4. Fresh registration challenge taken with correct user succeeds
    let (_challenge2, reg_state2) = state.webauthn.start_passkey_registration(
        Uuid::new_v4(),
        "user_42",
        "User 42",
        None,
    )?;
    let id2 = state.passkey.store_registration(user_id, reg_state2).await;
    let success = state.passkey.take_registration(&id2, user_id).await;
    assert!(success.is_ok());

    // 5. Non-existent UUID fails
    let random_id = Uuid::new_v4();
    let nonexistent =
        state.passkey.take_registration(&random_id, user_id).await;
    assert!(matches!(
        nonexistent,
        Err(crate::api::Error::BadRequest(
            "Challenge expired or invalid"
        ))
    ));

    Ok(())
}
