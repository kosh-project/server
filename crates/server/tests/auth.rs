#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]

mod boilerplate;
use std::time::Duration;

use boilerplate::{solve, with_sandbox_env};
use hyper::StatusCode;
use serde_json::{Value, json};
use webdav_server::model::{session::Session, user::User};

async fn pace_auth() {
    tokio::time::sleep(Duration::from_millis(2100)).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_recovery_endpoint_validation_e2e() -> anyhow::Result<()> {
    with_sandbox_env("webdav_recovery_val_test", async move |ctx| {
        // Create User 1 and User 2
        let id1 = [1u8; 32];
        let id2 = [2u8; 32];
        let initial_key1 = [10u8; 60];
        let initial_key2 = [20u8; 60];

        User::create(&ctx.db, &id1, "verifier_1".into(), &initial_key1).await?;
        User::create(&ctx.db, &id2, "verifier_2".into(), &initial_key2).await?;

        let u1_token = Session::create(&ctx.db, 1, &ctx.secret).await?;
        let recovery_endpoint =
            format!("{}/api/v1/auth/recovery", ctx.base_url);

        // 1. Unauthenticated request to recovery returns 401
        let unauth_resp = ctx
            .client
            .put(&recovery_endpoint)
            .json(&json!({}))
            .send()
            .await?;
        assert_eq!(unauth_resp.status(), StatusCode::UNAUTHORIZED);

        // 2. Invalid bearer token returns 401
        let invalid_resp = ctx
            .client
            .put(&recovery_endpoint)
            .header("Authorization", "Bearer invalid_token")
            .json(&json!({}))
            .send()
            .await?;
        assert_eq!(invalid_resp.status(), StatusCode::UNAUTHORIZED);

        // 3. Invalid identity_hash length (not 64 hex chars) returns 400
        let bad_id_payload = json!({
            "identity_hash": "short_hash",
            "auth_verifier": "new_verifier",
            "wrapped_vault_key": hex::encode([30u8; 60]),
            "revoke_other_sessions": false
        });
        let bad_id_resp = ctx
            .client
            .put(&recovery_endpoint)
            .header("Authorization", format!("Bearer {u1_token}"))
            .json(&bad_id_payload)
            .send()
            .await?;
        assert_eq!(bad_id_resp.status(), StatusCode::BAD_REQUEST);

        // 4. Invalid wrapped_vault_key length (not 120 hex chars) returns 400
        let bad_key_payload = json!({
            "identity_hash": hex::encode([3u8; 32]),
            "auth_verifier": "new_verifier",
            "wrapped_vault_key": "aabbcc",
            "revoke_other_sessions": false
        });
        let bad_key_resp = ctx
            .client
            .put(&recovery_endpoint)
            .header("Authorization", format!("Bearer {u1_token}"))
            .json(&bad_key_payload)
            .send()
            .await?;
        assert_eq!(bad_key_resp.status(), StatusCode::BAD_REQUEST);

        // 5. Unique violation: attempting to rotate User 1 to User 2's identity hash returns 409 Conflict
        let conflict_payload = json!({
            "identity_hash": hex::encode(id2),
            "auth_verifier": "new_verifier",
            "wrapped_vault_key": hex::encode([30u8; 60]),
            "revoke_other_sessions": false
        });
        let conflict_resp = ctx
            .client
            .put(&recovery_endpoint)
            .header("Authorization", format!("Bearer {u1_token}"))
            .json(&conflict_payload)
            .send()
            .await?;
        assert_eq!(conflict_resp.status(), StatusCode::CONFLICT);

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_recovery_endpoint_rotation_and_session_revocation_e2e()
-> anyhow::Result<()> {
    with_sandbox_env("webdav_recovery_e2e_test", async move |ctx| {
        // Create User 1 and User 2
        let id1 = [1u8; 32];
        let id2 = [2u8; 32];
        let initial_key1 = [10u8; 60];
        let initial_key2 = [20u8; 60];

        User::create(&ctx.db, &id1, "verifier_1".into(), &initial_key1).await?;
        User::create(&ctx.db, &id2, "verifier_2".into(), &initial_key2).await?;

        // Create 2 sessions for User 1, 1 for User 2
        let u1_token1 = Session::create(&ctx.db, 1, &ctx.secret).await?;
        let u1_token2 = Session::create(&ctx.db, 1, &ctx.secret).await?;
        let u2_token = Session::create(&ctx.db, 2, &ctx.secret).await?;

        let recovery_endpoint =
            format!("{}/api/v1/auth/recovery", ctx.base_url);
        let passkey_list_endpoint =
            format!("{}/api/v1/auth/passkey", ctx.base_url);

        // 1. Rotate recovery with revoke_other_sessions = false using u1_token1
        let rotated_key_a = [30u8; 60];
        let rotated_id_a = [3u8; 32];
        let valid_payload_keep = json!({
            "identity_hash": hex::encode(rotated_id_a),
            "auth_verifier": "verifier_1_rotated",
            "wrapped_vault_key": hex::encode(rotated_key_a),
            "revoke_other_sessions": false
        });
        let ok_keep_resp = ctx
            .client
            .put(&recovery_endpoint)
            .header("Authorization", format!("Bearer {u1_token1}"))
            .json(&valid_payload_keep)
            .send()
            .await?;
        assert_eq!(ok_keep_resp.status(), StatusCode::OK);

        // Both u1_token1 and u1_token2 remain valid
        let t1_resp = ctx
            .client
            .get(&passkey_list_endpoint)
            .header("Authorization", format!("Bearer {u1_token1}"))
            .send()
            .await?;
        assert_eq!(t1_resp.status(), StatusCode::OK);

        let t2_resp = ctx
            .client
            .get(&passkey_list_endpoint)
            .header("Authorization", format!("Bearer {u1_token2}"))
            .send()
            .await?;
        assert_eq!(t2_resp.status(), StatusCode::OK);

        // 2. Rotate recovery with revoke_other_sessions = true using u1_token1
        let rotated_key_b = [40u8; 60];
        let rotated_id_b = [4u8; 32];
        let valid_payload_revoke = json!({
            "identity_hash": hex::encode(rotated_id_b),
            "auth_verifier": "verifier_1_final",
            "wrapped_vault_key": hex::encode(rotated_key_b),
            "revoke_other_sessions": true
        });
        let ok_revoke_resp = ctx
            .client
            .put(&recovery_endpoint)
            .header("Authorization", format!("Bearer {u1_token1}"))
            .json(&valid_payload_revoke)
            .send()
            .await?;
        assert_eq!(ok_revoke_resp.status(), StatusCode::OK);

        // Active session u1_token1 is preserved
        let t1_after = ctx
            .client
            .get(&passkey_list_endpoint)
            .header("Authorization", format!("Bearer {u1_token1}"))
            .send()
            .await?;
        assert_eq!(t1_after.status(), StatusCode::OK);

        // Other session u1_token2 was REVOKED
        let t2_after = ctx
            .client
            .get(&passkey_list_endpoint)
            .header("Authorization", format!("Bearer {u1_token2}"))
            .send()
            .await?;
        assert_eq!(t2_after.status(), StatusCode::UNAUTHORIZED);

        // User 2's session is untouched
        let u2_after = ctx
            .client
            .get(&passkey_list_endpoint)
            .header("Authorization", format!("Bearer {u2_token}"))
            .send()
            .await?;
        assert_eq!(u2_after.status(), StatusCode::OK);

        // Database record reflects rotated wrapped_vault_key
        let updated_user = User::find_by_id(&ctx.db, 1).await?.unwrap();
        assert_eq!(updated_user.wrapped_vault_key, rotated_key_b.to_vec());

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_register_validation_vault_envelope_e2e() -> anyhow::Result<()> {
    with_sandbox_env("webdav_register_val_e2e_test", async move |ctx| {
        let challenge_endpoint = format!("{}/api/auth/challenge", ctx.base_url);
        let register_endpoint = format!("{}/api/auth/register", ctx.base_url);
        let user_id_hash = hex::encode([0x66u8; 32]);

        // 1. Get challenge for registration
        let chal_resp = ctx
            .client
            .get(format!("{challenge_endpoint}?action=register"))
            .send()
            .await?;
        assert_eq!(chal_resp.status(), StatusCode::OK);
        let chal_body: Value = chal_resp.json().await?;
        let challenge = chal_body["challenge"].as_str().unwrap();

        let solved_header = solve(challenge, &user_id_hash);

        // 2. Register with invalid wrapped_vault_key length returns 400
        let bad_reg_payload = json!({
            "auth_verifier": "argon_hash_1",
            "wrapped_vault_key": "1234abcd"
        });
        let bad_reg_resp = ctx
            .client
            .post(&register_endpoint)
            .header("X-Hashcash", &solved_header)
            .json(&bad_reg_payload)
            .send()
            .await?;
        assert_eq!(bad_reg_resp.status(), StatusCode::BAD_REQUEST);

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_register_and_login_vault_envelope_e2e() -> anyhow::Result<()> {
    with_sandbox_env("webdav_auth_envelope_e2e_test", async move |ctx| {
        let challenge_endpoint = format!("{}/api/auth/challenge", ctx.base_url);
        let register_endpoint = format!("{}/api/auth/register", ctx.base_url);
        let login_endpoint = format!("{}/api/auth/login", ctx.base_url);
        let passkey_list_endpoint =
            format!("{}/api/v1/auth/passkey", ctx.base_url);

        let user_id_hash = hex::encode([0x55u8; 32]);
        let vault_key_hex = hex::encode([0x77u8; 60]);

        // 1. Get challenge for registration
        let chal_resp = ctx
            .client
            .get(format!("{challenge_endpoint}?action=register"))
            .send()
            .await?;
        assert_eq!(chal_resp.status(), StatusCode::OK);
        let chal_body: Value = chal_resp.json().await?;
        let challenge = chal_body["challenge"].as_str().unwrap();

        // 2. Solve challenge
        let solved_header = solve(challenge, &user_id_hash);

        // 3. Register with valid wrapped_vault_key (120 hex chars) returns 201 Created
        let good_reg_payload = json!({
            "auth_verifier": "argon_hash_1",
            "wrapped_vault_key": vault_key_hex
        });
        let good_reg_resp = ctx
            .client
            .post(&register_endpoint)
            .header("X-Hashcash", &solved_header)
            .json(&good_reg_payload)
            .send()
            .await?;
        assert_eq!(good_reg_resp.status(), StatusCode::CREATED);

        // 4. Pace before login challenge to let auth IP limiter replenish
        pace_auth().await;

        // 5. Get challenge for login
        let login_chal_resp = ctx
            .client
            .get(format!("{challenge_endpoint}?action=login"))
            .send()
            .await?;
        assert_eq!(login_chal_resp.status(), StatusCode::OK);
        let login_chal_body: Value = login_chal_resp.json().await?;
        let login_challenge = login_chal_body["challenge"].as_str().unwrap();

        let login_pow = solve(login_challenge, &user_id_hash);

        // 6. Pace before login post to let auth IP limiter replenish
        pace_auth().await;

        // 7. Login with correct verifier returns 200 with token and matching wrapped_vault_key
        let correct_login_payload = json!({
            "auth_verifier": "argon_hash_1"
        });
        let correct_login_resp = ctx
            .client
            .post(&login_endpoint)
            .header("X-Hashcash", &login_pow)
            .json(&correct_login_payload)
            .send()
            .await?;
        assert_eq!(correct_login_resp.status(), StatusCode::OK);

        let login_body: Value = correct_login_resp.json().await?;
        let session_token = login_body["token"].as_str().unwrap();
        assert_eq!(login_body["wrapped_vault_key"], vault_key_hex);

        // 8. Session token is valid for authenticated routes
        let auth_resp = ctx
            .client
            .get(&passkey_list_endpoint)
            .header("Authorization", format!("Bearer {session_token}"))
            .send()
            .await?;
        assert_eq!(auth_resp.status(), StatusCode::OK);

        Ok(())
    })
    .await
}
