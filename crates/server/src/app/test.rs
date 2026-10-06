#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use super::*;
use crate::auth::Secret;
use sqlx::SqlitePool;
use std::path::Path;

#[tokio::test]
async fn app_state_builder_builds_valid_state() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("db connect");
    let secret = Secret::random();
    let vault_path = "/tmp/test_vault";

    let state = AppStateBuilder::new()
        .vault_path(vault_path)
        .db(pool)
        .secret(secret)
        .build();

    assert_eq!(state.vault_path(), Path::new(vault_path));
    assert_eq!(state.secret.key(), secret.key());
}

#[tokio::test]
#[should_panic(expected = "FATAL: vault_path is required!")]
async fn app_state_builder_panics_on_missing_vault_path() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("db connect");
    let secret = Secret::random();

    let _ = AppStateBuilder::new()
        .db(pool)
        .secret(secret)
        .build();
}

#[test]
#[should_panic(expected = "FATAL: database pool is required!")]
fn app_state_builder_panics_on_missing_db() {
    let secret = Secret::random();

    let _ = AppStateBuilder::new()
        .vault_path("/tmp")
        .secret(secret)
        .build();
}

#[tokio::test]
#[should_panic(expected = "FATAL: no secret provided??")]
async fn app_state_builder_panics_on_missing_secret() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("db connect");

    let _ = AppStateBuilder::new()
        .vault_path("/tmp")
        .db(pool)
        .build();
}
