#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]

mod boilerplate;
use boilerplate::with_sandbox_env;
use hyper::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;
use webdav_server::model::{passkey::Record, session::Session, user::User};

async fn pace() {
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_passkey_public_login_endpoints_e2e() -> anyhow::Result<()> {
    with_sandbox_env("webdav_passkey_public_test", async move |ctx| {
        let start_endpoint =
            format!("{}/api/auth/passkey/login/start", ctx.base_url);
        let finish_endpoint =
            format!("{}/api/auth/passkey/login/finish", ctx.base_url);

        // 1. POST /api/auth/passkey/login/start returns challenge_id and challenge
        let resp = ctx.client.post(&start_endpoint).send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = resp.json().await?;
        assert!(body.get("challenge_id").is_some());
        let challenge_id_str = body["challenge_id"].as_str().unwrap();
        assert!(Uuid::parse_str(challenge_id_str).is_ok());

        assert!(body.get("challenge").is_some());
        assert!(body["challenge"].get("publicKey").is_some());

        // 2. POST /api/auth/passkey/login/finish with nonexistent challenge_id returns 400
        let finish_payload = json!({
            "challenge_id": Uuid::new_v4().to_string(),
            "credential": {
                "id": "abc",
                "rawId": "abc",
                "response": {
                    "authenticatorData": "AAAA",
                    "clientDataJSON": "AAAA",
                    "signature": "AAAA"
                },
                "type": "public-key"
            }
        });

        let fail_resp = ctx
            .client
            .post(&finish_endpoint)
            .json(&finish_payload)
            .send()
            .await?;
        assert_eq!(fail_resp.status(), StatusCode::BAD_REQUEST);

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_passkey_protected_endpoints_auth_guard_e2e() -> anyhow::Result<()>
{
    with_sandbox_env("webdav_passkey_auth_guard_test", async move |ctx| {
        let register_start =
            format!("{}/api/v1/auth/passkey/register/start", ctx.base_url);
        let register_finish =
            format!("{}/api/v1/auth/passkey/register/finish", ctx.base_url);
        let list_endpoint = format!("{}/api/v1/auth/passkey", ctx.base_url);
        let delete_endpoint =
            format!("{}/api/v1/auth/passkey/01020304", ctx.base_url);

        // All protected endpoints must return 401 without Authorization header
        let r1 = ctx.client.post(&register_start).send().await?;
        assert_eq!(r1.status(), StatusCode::UNAUTHORIZED);

        let r2 = ctx
            .client
            .post(&register_finish)
            .json(&json!({}))
            .send()
            .await?;
        assert_eq!(r2.status(), StatusCode::UNAUTHORIZED);

        let r3 = ctx.client.get(&list_endpoint).send().await?;
        assert_eq!(r3.status(), StatusCode::UNAUTHORIZED);

        let r4 = ctx.client.delete(&delete_endpoint).send().await?;
        assert_eq!(r4.status(), StatusCode::UNAUTHORIZED);

        // Invalid bearer token returns 401
        let r5 = ctx
            .client
            .get(&list_endpoint)
            .header("Authorization", "Bearer invalid_or_forged_token")
            .send()
            .await?;
        assert_eq!(r5.status(), StatusCode::UNAUTHORIZED);

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_passkey_register_flow_validation_e2e() -> anyhow::Result<()> {
    with_sandbox_env("webdav_passkey_register_flow_test", async move |ctx| {
        // Create user & session
        User::create(&ctx.db, &[1u8; 32], "test_verifier".into(), &[0u8; 60])
            .await?;
        let user_id = 1;
        let token = Session::create(&ctx.db, user_id, &ctx.secret).await?;

        let start_endpoint =
            format!("{}/api/v1/auth/passkey/register/start", ctx.base_url);
        let finish_endpoint =
            format!("{}/api/v1/auth/passkey/register/finish", ctx.base_url);

        // 1. Authenticated register/start returns 200 with challenge_id & challenge
        let resp = ctx
            .client
            .post(&start_endpoint)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = resp.json().await?;
        let challenge_id_str = body["challenge_id"].as_str().unwrap();
        assert!(Uuid::parse_str(challenge_id_str).is_ok());
        assert!(body["challenge"]["publicKey"].is_object());

        // 2. register/finish with invalid metadata (bad hex) returns 400
        let bad_hex_payload = json!({
            "challenge_id": challenge_id_str,
            "credential": {
                "id": "abc",
                "rawId": "abc",
                "response": {
                    "attestationObject": "AAAA",
                    "clientDataJSON": "AAAA"
                },
                "type": "public-key"
            },
            "encrypted_metadata": "this_is_not_hex!"
        });
        let bad_hex_resp = ctx
            .client
            .post(&finish_endpoint)
            .header("Authorization", format!("Bearer {token}"))
            .json(&bad_hex_payload)
            .send()
            .await?;
        assert_eq!(bad_hex_resp.status(), StatusCode::BAD_REQUEST);

        // 3. register/finish with oversized metadata (> 2048 chars) returns 400
        let oversized_hex = "ff".repeat(1025);
        let oversized_payload = json!({
            "challenge_id": challenge_id_str,
            "credential": {
                "id": "abc",
                "rawId": "abc",
                "response": {
                    "attestationObject": "AAAA",
                    "clientDataJSON": "AAAA"
                },
                "type": "public-key"
            },
            "encrypted_metadata": oversized_hex
        });
        let oversized_resp = ctx
            .client
            .post(&finish_endpoint)
            .header("Authorization", format!("Bearer {token}"))
            .json(&oversized_payload)
            .send()
            .await?;
        assert_eq!(oversized_resp.status(), StatusCode::BAD_REQUEST);

        // 4. register/finish with unknown challenge_id returns 400
        let unknown_challenge_payload = json!({
            "challenge_id": Uuid::new_v4().to_string(),
            "credential": {
                "id": "abc",
                "rawId": "abc",
                "response": {
                    "attestationObject": "AAAA",
                    "clientDataJSON": "AAAA"
                },
                "type": "public-key"
            },
            "encrypted_metadata": "aabbccdd"
        });
        let unknown_resp = ctx
            .client
            .post(&finish_endpoint)
            .header("Authorization", format!("Bearer {token}"))
            .json(&unknown_challenge_payload)
            .send()
            .await?;
        assert_eq!(unknown_resp.status(), StatusCode::BAD_REQUEST);

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_passkey_crud_lifecycle_e2e() -> anyhow::Result<()> {
    with_sandbox_env("webdav_passkey_crud_lifecycle_test", async move |ctx| {
        // Create user 1 and user 2
        User::create(&ctx.db, &[2u8; 32], "v1".into(), &[0u8; 60]).await?;
        User::create(&ctx.db, &[3u8; 32], "v2".into(), &[0u8; 60]).await?;
        let u1_token = Session::create(&ctx.db, 1, &ctx.secret).await?;
        let u2_token = Session::create(&ctx.db, 2, &ctx.secret).await?;

        let list_endpoint = format!("{}/api/v1/auth/passkey", ctx.base_url);

        // 1. Initial list for user 1 is empty
        let empty_resp = ctx
            .client
            .get(&list_endpoint)
            .header("Authorization", format!("Bearer {u1_token}"))
            .send()
            .await?;
        assert_eq!(empty_resp.status(), StatusCode::OK);
        let empty_list: Vec<Value> = empty_resp.json().await?;
        assert_eq!(empty_list.len(), 0);

        // 2. Populate passkey records for user 1 and user 2
        let cred_u1_a = vec![0x11, 0x22, 0x33];
        let cred_u1_b = vec![0x44, 0x55, 0x66];
        let cred_u2 = vec![0x77, 0x88, 0x99];

        Record::create(
            &ctx.db,
            &Record {
                credential_id: cred_u1_a.clone(),
                user_id: 1,
                passkey_data: b"data1".to_vec(),
                counter: 1,
                encrypted_metadata: vec![0xaa, 0xbb],
                created_at: 100,
                last_used: 10,
            },
        )
        .await?;

        Record::create(
            &ctx.db,
            &Record {
                credential_id: cred_u1_b.clone(),
                user_id: 1,
                passkey_data: b"data2".to_vec(),
                counter: 2,
                encrypted_metadata: vec![0xcc, 0xdd],
                created_at: 200,
                last_used: 20,
            },
        )
        .await?;

        Record::create(
            &ctx.db,
            &Record {
                credential_id: cred_u2.clone(),
                user_id: 2,
                passkey_data: b"data3".to_vec(),
                counter: 0,
                encrypted_metadata: vec![0xee, 0xff],
                created_at: 150,
                last_used: 0,
            },
        )
        .await?;

        // 3. List user 1's passkeys: ordered created_at DESC (cred_u1_b first)
        pace().await;
        let list_resp = ctx
            .client
            .get(&list_endpoint)
            .header("Authorization", format!("Bearer {u1_token}"))
            .send()
            .await?;
        assert_eq!(list_resp.status(), StatusCode::OK);
        let list: Vec<Value> = list_resp.json().await?;
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["credential_id"], hex::encode(&cred_u1_b));
        assert_eq!(list[0]["encrypted_metadata"], "ccdd");
        assert_eq!(list[0]["created_at"], 200);
        assert_eq!(list[0]["last_used"], 20);

        assert_eq!(list[1]["credential_id"], hex::encode(&cred_u1_a));
        assert_eq!(list[1]["encrypted_metadata"], "aabb");
        assert_eq!(list[1]["created_at"], 100);
        assert_eq!(list[1]["last_used"], 10);

        // 4. Verify user 1's register/start now excludes user 1's 2 credentials
        let start_endpoint =
            format!("{}/api/v1/auth/passkey/register/start", ctx.base_url);
        pace().await;
        let reg_start_resp = ctx
            .client
            .post(&start_endpoint)
            .header("Authorization", format!("Bearer {u1_token}"))
            .send()
            .await?;
        assert_eq!(reg_start_resp.status(), StatusCode::OK);
        let start_body: Value = reg_start_resp.json().await?;
        let excluded =
            start_body["challenge"]["publicKey"]["excludeCredentials"]
                .as_array()
                .unwrap();
        assert_eq!(excluded.len(), 2);

        // 5. Delete with invalid hex returns 400
        let u1_del_token = Session::create(&ctx.db, 1, &ctx.secret).await?;
        let bad_hex_del =
            format!("{}/api/v1/auth/passkey/not_valid_hex", ctx.base_url);
        let del_bad = ctx
            .client
            .delete(&bad_hex_del)
            .header("Authorization", format!("Bearer {u1_del_token}"))
            .send()
            .await?;
        assert_eq!(del_bad.status(), StatusCode::BAD_REQUEST);

        // 6. Delete nonexistent credential returns 404
        let nonexistent_del =
            format!("{}/api/v1/auth/passkey/deadbeef0011", ctx.base_url);
        pace().await;
        let del_missing = ctx
            .client
            .delete(&nonexistent_del)
            .header("Authorization", format!("Bearer {u1_del_token}"))
            .send()
            .await?;
        assert_eq!(del_missing.status(), StatusCode::NOT_FOUND);

        // 7. User 1 tries to delete User 2's passkey -> 404 NOT_FOUND
        let u2_cred_hex = hex::encode(&cred_u2);
        let cross_del =
            format!("{}/api/v1/auth/passkey/{u2_cred_hex}", ctx.base_url);
        pace().await;
        let del_cross = ctx
            .client
            .delete(&cross_del)
            .header("Authorization", format!("Bearer {u1_del_token}"))
            .send()
            .await?;
        assert_eq!(del_cross.status(), StatusCode::NOT_FOUND);

        // Verify User 2's passkey is still intact
        let u2_list_resp = ctx
            .client
            .get(&list_endpoint)
            .header("Authorization", format!("Bearer {u2_token}"))
            .send()
            .await?;
        let u2_list: Vec<Value> = u2_list_resp.json().await?;
        assert_eq!(u2_list.len(), 1);

        // 8. User 1 deletes cred_u1_a -> 204 NO_CONTENT
        let u1_a_hex = hex::encode(&cred_u1_a);
        let own_del =
            format!("{}/api/v1/auth/passkey/{u1_a_hex}", ctx.base_url);
        pace().await;
        let del_success = ctx
            .client
            .delete(&own_del)
            .header("Authorization", format!("Bearer {u1_del_token}"))
            .send()
            .await?;
        assert_eq!(del_success.status(), StatusCode::NO_CONTENT);

        // Re-deleting cred_u1_a now returns 404
        pace().await;
        let del_again = ctx
            .client
            .delete(&own_del)
            .header("Authorization", format!("Bearer {u1_del_token}"))
            .send()
            .await?;
        assert_eq!(del_again.status(), StatusCode::NOT_FOUND);

        // User 1 list now has exactly 1 passkey (cred_u1_b)
        pace().await;
        let list_after_del = ctx
            .client
            .get(&list_endpoint)
            .header("Authorization", format!("Bearer {u1_token}"))
            .send()
            .await?;
        let list_after: Vec<Value> = list_after_del.json().await?;
        assert_eq!(list_after.len(), 1);
        assert_eq!(list_after[0]["credential_id"], hex::encode(&cred_u1_b));

        Ok(())
    })
    .await
}
