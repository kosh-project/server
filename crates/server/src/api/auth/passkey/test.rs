#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unnecessary_wraps)]

use super::*;
use crate::api::Error::BadRequest;
use anyhow::{Context, Result};
use uuid::Uuid;

#[test]
fn user_id_and_handle_bijection_roundtrip() -> Result<()> {
    let test_ids = [0, 1, 2, 42, 100_000, 1_000_000_000, i64::MAX];

    for &id in &test_ids {
        let handle = user_id_to_handle(id)
            .context("user_id_to_handle succeeds for non-negative")?;
        let roundtrip =
            handle_to_user_id(&handle).context("handle_to_user_id succeeds")?;
        assert_eq!(id, roundtrip);
    }

    Ok(())
}

#[test]
fn user_id_to_handle_negative_fails() {
    assert!(user_id_to_handle(-1).is_err());
    assert!(user_id_to_handle(-42).is_err());
    assert!(user_id_to_handle(i64::MIN).is_err());
}

#[test]
fn handle_to_user_id_out_of_bounds_fails() {
    // Upper bits set (higher than u64::MAX)
    let handle_large = Uuid::from_u128(u128::MAX);
    let res = handle_to_user_id(&handle_large);
    assert!(matches!(
        res,
        Err(BadRequest("Invalid user handle integer bounds"))
    ));

    // Exceeding i64::MAX using safe unsigned_abs conversion
    let max_u128 = u128::from(i64::MAX.unsigned_abs()) + 1;
    let handle_above_i64 = Uuid::from_u128(max_u128);
    let res = handle_to_user_id(&handle_above_i64);
    assert!(matches!(
        res,
        Err(BadRequest("Invalid user handle integer bounds"))
    ));

    // 1 << 64
    let handle_u64_overflow = Uuid::from_u128(1 << 64);
    let res = handle_to_user_id(&handle_u64_overflow);
    assert!(matches!(
        res,
        Err(BadRequest("Invalid user handle integer bounds"))
    ));
}

fn make_request_with_metadata(
    metadata: &str,
) -> Result<register::finish::Request> {
    let json = serde_json::json!({
        "challenge_id": "00000000-0000-0000-0000-000000000000",
        "credential": {
            "id": "AfJfonHsXY_f7_gFmV1dI473Ce--_g0tHhdXUoh7JmMn0gzhYUtU9bFqpCgSljjwJxEXkjzb-11ulePZyI0RiyQ",
            "rawId": "AfJfonHsXY_f7_gFmV1dI473Ce--_g0tHhdXUoh7JmMn0gzhYUtU9bFqpCgSljjwJxEXkjzb-11ulePZyI0RiyQ",
            "response": {
                "attestationObject": "o2NmbXRkbm9uZWdhdHRTdG10oGhhdXRoRGF0YVjFarm78N-aFvkduzO7sTL6-dF8eCxIJsbscOzuWNl-9SpFAAAAAAAAAAAAAAAAAAAAAAAAAAAAQQHyX6Jx7F2P3-_4BZldXSOO9wnvvv4NLR4XV1KIeyZjJ9IM4WFLVPWxaqQoEpY48CcRF5I82_tdbpXj2ciNEYskpQECAyYgASFYIE_9awy66uhXZ6hIzPAW2AzIrTMZ7kyC2jtZe0zuH_pOIlggFbNKhOSt8-prIx0snKRqcxULtc2u1rzUUf47g1PxTcU",
                "clientDataJSON": "eyJ0eXBlIjoid2ViYXV0aG4uY3JlYXRlIiwiY2hhbGxlbmdlIjoidF9XZTEzMU5wd2xseVBMMHgyNmJ6WmdrRjVmX1h2QTdPY2I0Yjk4emx4TSIsIm9yaWdpbiI6Imh0dHBzOlwvXC93ZWJhdXRobi5maXJzdHllYXIuaWQuYXUiLCJhbmRyb2lkUGFja2FnZU5hbWUiOiJvcmcubW96aWxsYS5maXJlZm94In0"
            },
            "type": "public-key"
        },
        "encrypted_metadata": metadata
    });
    let req =
        serde_json::from_value(json).context("valid finish::Request json")?;
    Ok(req)
}

#[test]
fn parse_metadata_valid() -> Result<()> {
    // Empty metadata
    let req = make_request_with_metadata("")?;
    let parsed = req.parse_metadata().context("empty metadata is valid")?;
    assert_eq!(parsed, Vec::<u8>::new());

    // Valid hex
    let req = make_request_with_metadata("deadbeef")?;
    let parsed = req.parse_metadata().context("valid hex")?;
    assert_eq!(parsed, vec![0xde, 0xad, 0xbe, 0xef]);

    // Exactly 2048 hex characters (1024 bytes)
    let hex_2048 = "ab".repeat(1024);
    assert_eq!(hex_2048.len(), 2048);
    let req = make_request_with_metadata(&hex_2048)?;
    let parsed = req.parse_metadata().context("2048 hex chars is allowed")?;
    assert_eq!(parsed.len(), 1024);
    assert_eq!(parsed, vec![0xab; 1024]);

    Ok(())
}

#[test]
fn parse_metadata_exceeding_max_length_fails() -> Result<()> {
    // 2050 hex characters (exceeds 2048)
    let hex_2050 = "ab".repeat(1025);
    assert_eq!(hex_2050.len(), 2050);
    let req = make_request_with_metadata(&hex_2050)?;
    let res = req.parse_metadata();
    assert!(matches!(
        res,
        Err(BadRequest(
            "encrypted_metadata can't exceed 2048 hex chars (or 1024 bytes)"
        ))
    ));

    Ok(())
}

#[test]
fn parse_metadata_invalid_hex_fails() -> Result<()> {
    let req = make_request_with_metadata("not_a_valid_hex_string")?;
    let res = req.parse_metadata();
    assert!(matches!(
        res,
        Err(BadRequest("Invalid hex encrypted_metadata"))
    ));

    // Odd-length hex string
    let req = make_request_with_metadata("abc")?;
    let res = req.parse_metadata();
    assert!(matches!(
        res,
        Err(BadRequest("Invalid hex encrypted_metadata"))
    ));

    Ok(())
}

#[tokio::test]
async fn verify_and_update_passkey_corrupted_state_rejected() -> Result<()> {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .context("db connect")?;
    let secret = crate::auth::Secret::random();
    let state = crate::app::AppStateBuilder::new()
        .vault_path("/tmp/test_vault")
        .db(pool)
        .secret(secret)
        .build();

    let (_challenge, auth_state) =
        state.webauthn.start_discoverable_authentication()?;

    let corrupted_record = crate::model::passkey::Record {
        credential_id: vec![1, 2, 3],
        user_id: 1,
        passkey_data: b"not_valid_json_passkey_data".to_vec(),
        counter: 0,
        encrypted_metadata: vec![],
        created_at: 1000,
        last_used: 0,
    };

    let dummy_cred_json = serde_json::json!({
        "id": "abc",
        "rawId": "abc",
        "response": {
            "authenticatorData": "AAAA",
            "clientDataJSON": "AAAA",
            "signature": "AAAA"
        },
        "type": "public-key"
    });
    let credential = serde_json::from_value(dummy_cred_json)
        .context("parse dummy credential")?;

    let res = login::verify_and_update_passkey(
        &state,
        &corrupted_record,
        &credential,
        auth_state,
    )
    .await;

    match res {
        Err(err) => assert!(
            err.to_string()
                .contains("Corrupted passkey state in database")
        ),
        Ok(()) => anyhow::bail!("expected Err on corrupted passkey state"),
    }

    Ok(())
}
