#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

mod boilerplate;
use boilerplate::{solve, with_sandbox_env};
use hyper::StatusCode;
use serde_json::{Value, json};
use server::model::{session::Session, user::User};

async fn pace() {
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
}

async fn pace_auth() {
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
}

async fn get_pow_header(
    client: &reqwest::Client,
    base_url: &str,
) -> anyhow::Result<String> {
    let chal_resp = client
        .get(format!("{base_url}/api/auth/challenge?action=register"))
        .send()
        .await?;
    assert_eq!(chal_resp.status(), StatusCode::OK);
    let chal_body: Value = chal_resp.json().await?;
    let challenge = chal_body["challenge"]
        .as_str()
        .expect("challenge is string");
    let identity_hash = hex::encode([0x55u8; 32]);
    Ok(solve(challenge, &identity_hash))
}

#[tokio::test]
#[serial_test::serial]
async fn test_mailbox_auth_guard_and_unauthorized_access_e2e()
-> anyhow::Result<()> {
    with_sandbox_env("webdav_mailbox_auth_guard_test", async move |ctx| {
        let offer_endpoint = format!("{}/api/v1/devices/mailbox", ctx.base_url);
        let link_ticket_endpoint =
            format!("{}/api/v1/devices/link-ticket", ctx.base_url);
        let status_endpoint =
            format!("{}/api/v1/devices/mailbox/01234567/status", ctx.base_url);
        let fulfill_endpoint =
            format!("{}/api/v1/devices/mailbox/01234567/fulfill", ctx.base_url);

        let open_endpoint = format!("{}/api/auth/mailbox/open", ctx.base_url);
        let join_endpoint =
            format!("{}/api/auth/mailbox/01234567/join", ctx.base_url);
        let pickup_endpoint =
            format!("{}/api/auth/mailbox/01234567/pickup", ctx.base_url);

        let valid_payload = json!({
            "payload": "ab".repeat(60)
        });

        // 1. Missing Authorization header on protected routes returns 401 Unauthorized
        let r1 = ctx.client.post(&offer_endpoint).send().await?;
        assert_eq!(r1.status(), StatusCode::UNAUTHORIZED);

        let r2 = ctx.client.post(&link_ticket_endpoint).send().await?;
        assert_eq!(r2.status(), StatusCode::UNAUTHORIZED);

        let r3 = ctx.client.get(&status_endpoint).send().await?;
        assert_eq!(r3.status(), StatusCode::UNAUTHORIZED);

        let r4 = ctx
            .client
            .post(&fulfill_endpoint)
            .json(&valid_payload)
            .send()
            .await?;
        assert_eq!(r4.status(), StatusCode::UNAUTHORIZED);

        // 2. Invalid / forged Bearer token returns 401 Unauthorized
        let forged_header = "Bearer forged_invalid_session_token";

        let r5 = ctx
            .client
            .post(&offer_endpoint)
            .header("Authorization", forged_header)
            .send()
            .await?;
        assert_eq!(r5.status(), StatusCode::UNAUTHORIZED);

        let r6 = ctx
            .client
            .post(&link_ticket_endpoint)
            .header("Authorization", forged_header)
            .send()
            .await?;
        assert_eq!(r6.status(), StatusCode::UNAUTHORIZED);

        let r7 = ctx
            .client
            .get(&status_endpoint)
            .header("Authorization", forged_header)
            .send()
            .await?;
        assert_eq!(r7.status(), StatusCode::UNAUTHORIZED);

        let r8 = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", forged_header)
            .json(&valid_payload)
            .send()
            .await?;
        assert_eq!(r8.status(), StatusCode::UNAUTHORIZED);

        // 3. PoW-guarded unauthenticated routes reject requests missing X-Hashcash header
        pace_auth().await;
        let r9 = ctx
            .client
            .post(&open_endpoint)
            .json(&json!({ "pub_temp": "aa".repeat(32) }))
            .send()
            .await?;
        assert_eq!(r9.status(), StatusCode::UNAUTHORIZED);

        pace_auth().await;
        let r10 = ctx
            .client
            .post(&join_endpoint)
            .json(&json!({ "pub_temp": "bb".repeat(32) }))
            .send()
            .await?;
        assert_eq!(r10.status(), StatusCode::UNAUTHORIZED);

        // 4. Pickup requires Authorization header
        pace_auth().await;
        let r11 = ctx.client.get(&pickup_endpoint).send().await?;
        assert_eq!(r11.status(), StatusCode::UNAUTHORIZED);

        // 5. Pickup with forged or unknown polling token returns 404 (anti-enumeration)
        pace_auth().await;
        let r12 = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", "Bearer invalid_polling_token")
            .send()
            .await?;
        assert_eq!(r12.status(), StatusCode::NOT_FOUND);

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_e2e_flow_a_mobile_opens_desktop_scans() -> anyhow::Result<()> {
    with_sandbox_env("webdav_mailbox_flow_a_test", async move |ctx| {
        // Setup authenticated desktop session for User 1
        User::create(&ctx.db, &[1u8; 32], "test_verifier".into(), &[0u8; 60])
            .await?;
        let user_id = 1;
        let desktop_token =
            Session::create(&ctx.db, user_id, &ctx.secret).await?;
        let desktop_auth = format!("Bearer {desktop_token}");

        let open_endpoint = format!("{}/api/auth/mailbox/open", ctx.base_url);
        let link_ticket_endpoint =
            format!("{}/api/v1/devices/link-ticket", ctx.base_url);

        // 1. Mobile fetches PoW and opens an ephemeral mailbox
        pace_auth().await;
        let pow = get_pow_header(&ctx.client, &ctx.base_url).await?;

        pace_auth().await;
        let pub_temp_receiver = "aa".repeat(32);
        let open_resp = ctx
            .client
            .post(&open_endpoint)
            .header("X-Hashcash", &pow)
            .json(&json!({ "pub_temp": pub_temp_receiver }))
            .send()
            .await?;
        assert_eq!(open_resp.status(), StatusCode::OK);

        let open_body: Value = open_resp.json().await?;
        let mailbox_id = open_body["mailbox_id"]
            .as_str()
            .expect("mailbox_id is string");
        let polling_token = open_body["polling_token"]
            .as_str()
            .expect("polling_token is string");
        assert_eq!(mailbox_id.len(), 8);
        assert_eq!(polling_token.len(), 64);
        assert_eq!(open_body["expires_in"], 300);

        let pickup_endpoint =
            format!("{}/api/auth/mailbox/{mailbox_id}/pickup", ctx.base_url);
        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{mailbox_id}/fulfill",
            ctx.base_url
        );
        let receiver_auth = format!("Bearer {polling_token}");

        // 2. Mobile polls pending mailbox before fulfillment: 200 {"status": "pending"}
        pace_auth().await;
        let poll_resp = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &receiver_auth)
            .send()
            .await?;
        assert_eq!(poll_resp.status(), StatusCode::OK);
        let poll_body: Value = poll_resp.json().await?;
        assert_eq!(poll_body, json!({ "status": "pending" }));

        // 3. Desktop scans QR code and mints registration ticket
        pace().await;
        let ticket_resp = ctx
            .client
            .post(&link_ticket_endpoint)
            .header("Authorization", &desktop_auth)
            .send()
            .await?;
        assert_eq!(ticket_resp.status(), StatusCode::OK);
        let ticket_body: Value = ticket_resp.json().await?;
        let ticket = ticket_body["ticket"].as_str().expect("ticket is string");
        assert_eq!(ticket.len(), 64);
        assert_eq!(ticket_body["expires_in"], 300);

        // 4. Desktop deposits encrypted ECDH ciphertext containing registration ticket
        // Ciphertext payload >= 120 hex chars
        let ciphertext = format!("{ticket}{}", "ff".repeat(32));
        assert!(ciphertext.len() >= 120);

        pace().await;
        let fulfill_resp = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &desktop_auth)
            .json(&json!({ "payload": ciphertext }))
            .send()
            .await?;
        assert_eq!(fulfill_resp.status(), StatusCode::NO_CONTENT);

        // 5. Double fulfill rejection: fulfilling again returns 400 Bad Request
        pace().await;
        let double_fulfill = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &desktop_auth)
            .json(&json!({ "payload": ciphertext }))
            .send()
            .await?;
        assert_eq!(double_fulfill.status(), StatusCode::BAD_REQUEST);

        // 6. Mobile picks up payload
        pace_auth().await;
        let pickup_resp = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &receiver_auth)
            .send()
            .await?;
        assert_eq!(pickup_resp.status(), StatusCode::OK);
        let pickup_body: Value = pickup_resp.json().await?;
        assert_eq!(
            pickup_body,
            json!({
                "status": "fulfilled",
                "payload": ciphertext
            })
        );

        // 7. Single-use burn-after-reading: subsequent pickup returns 404 Not Found
        pace_auth().await;
        let post_burn = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &receiver_auth)
            .send()
            .await?;
        assert_eq!(post_burn.status(), StatusCode::NOT_FOUND);

        // 8. Mobile uses registration ticket to initiate passkey registration ceremony
        let reg_start_endpoint =
            format!("{}/api/auth/passkey/register/start", ctx.base_url);
        let reg_finish_endpoint =
            format!("{}/api/auth/passkey/register/finish", ctx.base_url);

        pace_auth().await;
        let start_resp = ctx
            .client
            .post(&reg_start_endpoint)
            .header("Authorization", format!("Bearer {ticket}"))
            .send()
            .await?;
        assert_eq!(start_resp.status(), StatusCode::OK);
        let start_body: Value = start_resp.json().await?;
        assert!(start_body.get("challenge_id").is_some());
        assert!(start_body.get("challenge").is_some());

        // Calling finish with invalid/unknown challenge demonstrates ticket authorized entry
        pace_auth().await;
        let fake_challenge = uuid::Uuid::new_v4().to_string();
        let finish_payload = json!({
            "challenge_id": fake_challenge,
            "credential": {
                "id": "mock_id",
                "rawId": "mock_id",
                "response": {
                    "attestationObject": "AAAA",
                    "clientDataJSON": "AAAA"
                },
                "type": "public-key"
            },
            "encrypted_metadata": "aabbccdd"
        });

        let finish_resp = ctx
            .client
            .post(&reg_finish_endpoint)
            .header("Authorization", format!("Bearer {ticket}"))
            .json(&finish_payload)
            .send()
            .await?;
        // Handler accepted the ticket; rejection is due to invalid WebAuthn credential payload
        assert_ne!(finish_resp.status(), StatusCode::UNAUTHORIZED);

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_e2e_flow_b_desktop_offers_phone_joins() -> anyhow::Result<()> {
    with_sandbox_env("webdav_mailbox_flow_b_test", async move |ctx| {
        // Desktop user (User 1)
        User::create(&ctx.db, &[2u8; 32], "test_verifier".into(), &[0u8; 60])
            .await?;
        let user_id = 1;
        let desktop_token =
            Session::create(&ctx.db, user_id, &ctx.secret).await?;
        let desktop_auth = format!("Bearer {desktop_token}");

        let offer_endpoint = format!("{}/api/v1/devices/mailbox", ctx.base_url);

        // 1. Desktop allocates pairing mailbox
        pace().await;
        let offer_resp = ctx
            .client
            .post(&offer_endpoint)
            .header("Authorization", &desktop_auth)
            .send()
            .await?;
        assert_eq!(offer_resp.status(), StatusCode::OK);
        let offer_body: Value = offer_resp.json().await?;
        let mailbox_id = offer_body["mailbox_id"]
            .as_str()
            .expect("mailbox_id is string");
        assert_eq!(mailbox_id.len(), 8);
        assert_eq!(offer_body["expires_in"], 300);

        let status_endpoint = format!(
            "{}/api/v1/devices/mailbox/{mailbox_id}/status",
            ctx.base_url
        );
        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{mailbox_id}/fulfill",
            ctx.base_url
        );
        let join_endpoint =
            format!("{}/api/auth/mailbox/{mailbox_id}/join", ctx.base_url);
        let pickup_endpoint =
            format!("{}/api/auth/mailbox/{mailbox_id}/pickup", ctx.base_url);

        // 2. Desktop polls status while waiting for receiver: 200 {"status": "waiting"}
        pace().await;
        let status_resp = ctx
            .client
            .get(&status_endpoint)
            .header("Authorization", &desktop_auth)
            .send()
            .await?;
        assert_eq!(status_resp.status(), StatusCode::OK);
        let status_body: Value = status_resp.json().await?;
        assert_eq!(status_body, json!({ "status": "waiting" }));

        // 3. Early fulfill before join returns 400 Bad Request ("Mailbox not ready")
        pace().await;
        let early_fulfill = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &desktop_auth)
            .json(&json!({ "payload": "ab".repeat(60) }))
            .send()
            .await?;
        assert_eq!(early_fulfill.status(), StatusCode::BAD_REQUEST);

        // 4. Phone solves PoW and joins with ephemeral public key
        pace_auth().await;
        let pow = get_pow_header(&ctx.client, &ctx.base_url).await?;

        let phone_pub_temp = "bb".repeat(32);
        pace_auth().await;
        let join_resp = ctx
            .client
            .post(&join_endpoint)
            .header("X-Hashcash", &pow)
            .json(&json!({ "pub_temp": phone_pub_temp }))
            .send()
            .await?;
        assert_eq!(join_resp.status(), StatusCode::OK);
        let join_body: Value = join_resp.json().await?;
        let token = join_body["token"].as_str().expect("token is string");
        assert_eq!(token.len(), 64);

        // 5. Double join rejection: subsequent join returns 400 Bad Request ("Mailbox already joined")
        pace_auth().await;
        let double_join = ctx
            .client
            .post(&join_endpoint)
            .header("X-Hashcash", &pow)
            .json(&json!({ "pub_temp": phone_pub_temp }))
            .send()
            .await?;
        assert_eq!(double_join.status(), StatusCode::BAD_REQUEST);

        // 6. Desktop polls status again: 200 {"status": "ready", "pub_temp": "<hex>"}
        pace().await;
        let ready_status = ctx
            .client
            .get(&status_endpoint)
            .header("Authorization", &desktop_auth)
            .send()
            .await?;
        assert_eq!(ready_status.status(), StatusCode::OK);
        let ready_body: Value = ready_status.json().await?;
        assert_eq!(
            ready_body,
            json!({
                "status": "ready",
                "pub_temp": phone_pub_temp
            })
        );

        // 7. Desktop fulfills mailbox with ciphertext
        let payload = "cafe".repeat(30);
        pace().await;
        let fulfill_resp = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &desktop_auth)
            .json(&json!({ "payload": payload }))
            .send()
            .await?;
        assert_eq!(fulfill_resp.status(), StatusCode::NO_CONTENT);

        // 8. Double fulfill rejection: fulfilling again returns 400 Bad Request
        let desktop_auth_2 = format!(
            "Bearer {}",
            Session::create(&ctx.db, user_id, &ctx.secret).await?
        );
        pace().await;
        let double_fulfill = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &desktop_auth_2)
            .json(&json!({ "payload": payload }))
            .send()
            .await?;
        assert_eq!(double_fulfill.status(), StatusCode::BAD_REQUEST);

        // 9. Phone picks up payload
        let phone_auth = format!("Bearer {token}");
        pace_auth().await;
        let pickup_resp = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &phone_auth)
            .send()
            .await?;
        assert_eq!(pickup_resp.status(), StatusCode::OK);
        let pickup_body: Value = pickup_resp.json().await?;
        assert_eq!(
            pickup_body,
            json!({
                "status": "fulfilled",
                "payload": payload
            })
        );

        // 10. Single-use burn-after-reading: subsequent pickup returns 404 Not Found
        pace_auth().await;
        let post_burn = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &phone_auth)
            .send()
            .await?;
        assert_eq!(post_burn.status(), StatusCode::NOT_FOUND);

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_mailbox_cross_tenant_isolation_e2e() -> anyhow::Result<()> {
    with_sandbox_env("webdav_mailbox_isolation_test", async move |ctx| {
        // Create User 1 & Session 1
        User::create(&ctx.db, &[10u8; 32], "v1".into(), &[0u8; 60]).await?;
        let u1_token = Session::create(&ctx.db, 1, &ctx.secret).await?;
        let u1_auth = format!("Bearer {u1_token}");

        // Create User 2 & Session 2
        User::create(&ctx.db, &[20u8; 32], "v2".into(), &[0u8; 60]).await?;
        let u2_token = Session::create(&ctx.db, 2, &ctx.secret).await?;
        let u2_auth = format!("Bearer {u2_token}");

        let offer_endpoint = format!("{}/api/v1/devices/mailbox", ctx.base_url);

        // User 1 creates mailbox
        pace().await;
        let create_resp = ctx
            .client
            .post(&offer_endpoint)
            .header("Authorization", &u1_auth)
            .send()
            .await?;
        assert_eq!(create_resp.status(), StatusCode::OK);
        let body: Value = create_resp.json().await?;
        let u1_mailbox_id = body["mailbox_id"].as_str().unwrap();

        let status_endpoint = format!(
            "{}/api/v1/devices/mailbox/{u1_mailbox_id}/status",
            ctx.base_url
        );
        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{u1_mailbox_id}/fulfill",
            ctx.base_url
        );
        let join_endpoint =
            format!("{}/api/auth/mailbox/{u1_mailbox_id}/join", ctx.base_url);
        let pickup_endpoint =
            format!("{}/api/auth/mailbox/{u1_mailbox_id}/pickup", ctx.base_url);

        let valid_payload = json!({
            "payload": "1234".repeat(30)
        });

        // 1. User 2 attempts to check status of User 1's mailbox -> 404 Not Found
        pace().await;
        let u2_status = ctx
            .client
            .get(&status_endpoint)
            .header("Authorization", &u2_auth)
            .send()
            .await?;
        assert_eq!(u2_status.status(), StatusCode::NOT_FOUND);

        // 2. User 2 attempts to fulfill User 1's mailbox -> 404 Not Found
        pace().await;
        let u2_fulfill = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &u2_auth)
            .json(&valid_payload)
            .send()
            .await?;
        assert_eq!(u2_fulfill.status(), StatusCode::NOT_FOUND);

        // 3. User 1 checks status: Untouched and Waiting
        pace().await;
        let u1_status = ctx
            .client
            .get(&status_endpoint)
            .header("Authorization", &u1_auth)
            .send()
            .await?;
        assert_eq!(u1_status.status(), StatusCode::OK);
        let u1_body: Value = u1_status.json().await?;
        assert_eq!(u1_body, json!({ "status": "waiting" }));

        // 4. Device joins User 1's mailbox
        pace_auth().await;
        let pow = get_pow_header(&ctx.client, &ctx.base_url).await?;
        pace_auth().await;
        let join_resp = ctx
            .client
            .post(&join_endpoint)
            .header("X-Hashcash", &pow)
            .json(&json!({ "pub_temp": "cc".repeat(32) }))
            .send()
            .await?;
        assert_eq!(join_resp.status(), StatusCode::OK);
        let join_body: Value = join_resp.json().await?;
        let token = join_body["token"].as_str().unwrap();

        // 5. User 2 still cannot check status or fulfill after join
        pace().await;
        let u2_status_post_join = ctx
            .client
            .get(&status_endpoint)
            .header("Authorization", &u2_auth)
            .send()
            .await?;
        assert_eq!(u2_status_post_join.status(), StatusCode::NOT_FOUND);

        pace().await;
        let u2_fulfill_post_join = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &u2_auth)
            .json(&valid_payload)
            .send()
            .await?;
        assert_eq!(u2_fulfill_post_join.status(), StatusCode::NOT_FOUND);

        // 6. User 1 fulfills normally
        pace().await;
        let u1_fulfill = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &u1_auth)
            .json(&valid_payload)
            .send()
            .await?;
        assert_eq!(u1_fulfill.status(), StatusCode::NO_CONTENT);

        // 7. Receiver picks up payload
        pace_auth().await;
        let pickup_resp = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await?;
        assert_eq!(pickup_resp.status(), StatusCode::OK);

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_mailbox_payload_validation_e2e() -> anyhow::Result<()> {
    with_sandbox_env("webdav_mailbox_validation_test", async move |ctx| {
        User::create(&ctx.db, &[50u8; 32], "v".into(), &[0u8; 60]).await?;
        let token = Session::create(&ctx.db, 1, &ctx.secret).await?;
        let auth = format!("Bearer {token}");

        let open_endpoint = format!("{}/api/auth/mailbox/open", ctx.base_url);
        let join_endpoint_mock =
            format!("{}/api/auth/mailbox/01234567/join", ctx.base_url);

        pace_auth().await;
        let pow = get_pow_header(&ctx.client, &ctx.base_url).await?;

        // 1. Open validation: pub_temp != 64 hex chars -> 400 Bad Request
        pace_auth().await;
        let open_short = ctx
            .client
            .post(&open_endpoint)
            .header("X-Hashcash", &pow)
            .json(&json!({ "pub_temp": "aa".repeat(31) })) // 62 chars
            .send()
            .await?;
        assert_eq!(open_short.status(), StatusCode::BAD_REQUEST);

        // 2. Open validation: pub_temp invalid hex -> 400 Bad Request
        pace_auth().await;
        let open_bad_hex = ctx
            .client
            .post(&open_endpoint)
            .header("X-Hashcash", &pow)
            .json(&json!({ "pub_temp": format!("{}zz", "aa".repeat(31)) }))
            .send()
            .await?;
        assert_eq!(open_bad_hex.status(), StatusCode::BAD_REQUEST);

        // 3. Join validation: pub_temp != 64 hex chars -> 400 Bad Request
        pace_auth().await;
        let join_short = ctx
            .client
            .post(&join_endpoint_mock)
            .header("X-Hashcash", &pow)
            .json(&json!({ "pub_temp": "bb".repeat(33) })) // 66 chars
            .send()
            .await?;
        assert_eq!(join_short.status(), StatusCode::BAD_REQUEST);

        // 4. Open valid mailbox to test fulfill validation
        pace_auth().await;
        let open_valid = ctx
            .client
            .post(&open_endpoint)
            .header("X-Hashcash", &pow)
            .json(&json!({ "pub_temp": "aa".repeat(32) }))
            .send()
            .await?;
        assert_eq!(open_valid.status(), StatusCode::OK);
        let open_body: Value = open_valid.json().await?;
        let mailbox_id = open_body["mailbox_id"].as_str().unwrap();

        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{mailbox_id}/fulfill",
            ctx.base_url
        );

        // 5. Fulfill under min length (118 hex chars) -> 400 Bad Request
        pace().await;
        let too_short = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth)
            .json(&json!({ "payload": "aa".repeat(59) }))
            .send()
            .await?;
        assert_eq!(too_short.status(), StatusCode::BAD_REQUEST);

        // 6. Fulfill over max length (8194 hex chars) -> 400 Bad Request
        pace().await;
        let too_long = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth)
            .json(&json!({ "payload": "bb".repeat(4097) }))
            .send()
            .await?;
        assert_eq!(too_long.status(), StatusCode::BAD_REQUEST);

        // 7. Fulfill odd length hex characters (121 chars) -> 400 Bad Request
        pace().await;
        let mut odd_hex = "cc".repeat(60);
        odd_hex.push('c');
        assert_eq!(odd_hex.len(), 121);
        let odd_resp = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth)
            .json(&json!({ "payload": odd_hex }))
            .send()
            .await?;
        assert_eq!(odd_resp.status(), StatusCode::BAD_REQUEST);

        // 8. Fulfill invalid hex characters (120 chars with 'zz') -> 400 Bad Request
        let auth2 = format!(
            "Bearer {}",
            Session::create(&ctx.db, 1, &ctx.secret).await?
        );
        pace().await;
        let mut invalid_hex = "dd".repeat(59);
        invalid_hex.push_str("zz");
        assert_eq!(invalid_hex.len(), 120);
        let invalid_resp = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth2)
            .json(&json!({ "payload": invalid_hex }))
            .send()
            .await?;
        assert_eq!(invalid_resp.status(), StatusCode::BAD_REQUEST);

        // 9. Fulfill empty payload -> 400 Bad Request
        pace().await;
        let empty_resp = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth2)
            .json(&json!({ "payload": "" }))
            .send()
            .await?;
        assert_eq!(empty_resp.status(), StatusCode::BAD_REQUEST);

        // 10. Exactly min boundary length (120 hex chars / 60 bytes) -> succeeds (204)
        let exact_min = "ee".repeat(60);
        assert_eq!(exact_min.len(), 120);
        pace().await;
        let min_resp = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth2)
            .json(&json!({ "payload": exact_min }))
            .send()
            .await?;
        assert_eq!(min_resp.status(), StatusCode::NO_CONTENT);

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_mailbox_nonexistent_mailbox_e2e() -> anyhow::Result<()> {
    with_sandbox_env("webdav_mailbox_nonexistent_test", async move |ctx| {
        User::create(&ctx.db, &[55u8; 32], "v".into(), &[0u8; 60]).await?;
        let token = Session::create(&ctx.db, 1, &ctx.secret).await?;
        let auth = format!("Bearer {token}");

        let fake_id = "00000000";
        let status_endpoint =
            format!("{}/api/v1/devices/mailbox/{fake_id}/status", ctx.base_url);
        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{fake_id}/fulfill",
            ctx.base_url
        );
        let pickup_endpoint =
            format!("{}/api/auth/mailbox/{fake_id}/pickup", ctx.base_url);
        let join_endpoint =
            format!("{}/api/auth/mailbox/{fake_id}/join", ctx.base_url);

        // Status returns 404
        pace().await;
        let missing_status = ctx
            .client
            .get(&status_endpoint)
            .header("Authorization", &auth)
            .send()
            .await?;
        assert_eq!(missing_status.status(), StatusCode::NOT_FOUND);

        // Fulfill returns 404
        pace().await;
        let missing_fulfill = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth)
            .json(&json!({ "payload": "ff".repeat(60) }))
            .send()
            .await?;
        assert_eq!(missing_fulfill.status(), StatusCode::NOT_FOUND);

        // Pickup returns 404
        pace_auth().await;
        let missing_pickup = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", "Bearer mock_token")
            .send()
            .await?;
        assert_eq!(missing_pickup.status(), StatusCode::NOT_FOUND);

        // Join returns 404
        pace_auth().await;
        let pow = get_pow_header(&ctx.client, &ctx.base_url).await?;
        pace_auth().await;
        let missing_join = ctx
            .client
            .post(&join_endpoint)
            .header("X-Hashcash", &pow)
            .json(&json!({ "pub_temp": "aa".repeat(32) }))
            .send()
            .await?;
        assert_eq!(missing_join.status(), StatusCode::NOT_FOUND);

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_mailbox_boundary_max_payload_e2e() -> anyhow::Result<()> {
    with_sandbox_env("webdav_mailbox_max_payload_test", async move |ctx| {
        User::create(&ctx.db, &[99u8; 32], "v".into(), &[0u8; 60]).await?;
        let token = Session::create(&ctx.db, 1, &ctx.secret).await?;
        let auth = format!("Bearer {token}");

        let open_endpoint = format!("{}/api/auth/mailbox/open", ctx.base_url);

        pace_auth().await;
        let pow = get_pow_header(&ctx.client, &ctx.base_url).await?;
        pace_auth().await;
        let open_resp = ctx
            .client
            .post(&open_endpoint)
            .header("X-Hashcash", &pow)
            .json(&json!({ "pub_temp": "aa".repeat(32) }))
            .send()
            .await?;
        assert_eq!(open_resp.status(), StatusCode::OK);
        let open_body: Value = open_resp.json().await?;
        let mailbox_id = open_body["mailbox_id"].as_str().unwrap();
        let polling_token = open_body["polling_token"].as_str().unwrap();

        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{mailbox_id}/fulfill",
            ctx.base_url
        );
        let pickup_endpoint =
            format!("{}/api/auth/mailbox/{mailbox_id}/pickup", ctx.base_url);

        // Exactly 8192 hex characters (4096 bytes)
        let max_payload = "0a".repeat(4096);
        assert_eq!(max_payload.len(), 8192);

        pace().await;
        let fulfill_resp = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth)
            .json(&json!({ "payload": max_payload }))
            .send()
            .await?;
        assert_eq!(fulfill_resp.status(), StatusCode::NO_CONTENT);

        pace_auth().await;
        let pickup_resp = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", format!("Bearer {polling_token}"))
            .send()
            .await?;
        assert_eq!(pickup_resp.status(), StatusCode::OK);
        let pickup_body: Value = pickup_resp.json().await?;
        assert_eq!(
            pickup_body,
            json!({
                "status": "fulfilled",
                "payload": max_payload
            })
        );

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_mailbox_concurrent_pickup_e2e() -> anyhow::Result<()> {
    with_sandbox_env("webdav_mailbox_concurrent_pickup", async move |ctx| {
        User::create(&ctx.db, &[77u8; 32], "v".into(), &[0u8; 60]).await?;
        let user_id = 1;
        let token = Session::create(&ctx.db, user_id, &ctx.secret).await?;
        let auth = format!("Bearer {token}");

        let open_endpoint = format!("{}/api/auth/mailbox/open", ctx.base_url);

        pace_auth().await;
        let pow = get_pow_header(&ctx.client, &ctx.base_url).await?;
        pace_auth().await;
        let open_resp = ctx
            .client
            .post(&open_endpoint)
            .header("X-Hashcash", &pow)
            .json(&json!({ "pub_temp": "aa".repeat(32) }))
            .send()
            .await?;
        assert_eq!(open_resp.status(), StatusCode::OK);
        let open_body: Value = open_resp.json().await?;
        let mailbox_id = open_body["mailbox_id"].as_str().unwrap();
        let polling_token = open_body["polling_token"].as_str().unwrap();

        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{mailbox_id}/fulfill",
            ctx.base_url
        );
        let pickup_endpoint =
            format!("{}/api/auth/mailbox/{mailbox_id}/pickup", ctx.base_url);

        let payload = "42".repeat(60);
        pace().await;
        let fulfill_resp = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth)
            .json(&json!({ "payload": payload }))
            .send()
            .await?;
        assert_eq!(fulfill_resp.status(), StatusCode::NO_CONTENT);

        // Concurrently attempt pickup with valid token
        let token_auth = format!("Bearer {polling_token}");
        pace_auth().await;
        let req1 = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &token_auth)
            .send();
        let req2 = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &token_auth)
            .send();

        let (res1, res2) = tokio::join!(req1, req2);
        let status1 = res1?.status();
        let status2 = res2?.status();

        let statuses = [status1, status2];
        assert!(
            statuses.contains(&StatusCode::OK),
            "One concurrent pickup must succeed with 200 OK"
        );
        assert!(
            statuses.contains(&StatusCode::NOT_FOUND),
            "The other concurrent pickup must receive 404 NOT_FOUND"
        );

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_mailbox_concurrent_fulfill_e2e() -> anyhow::Result<()> {
    with_sandbox_env("webdav_mailbox_concurrent_fulfill", async move |ctx| {
        User::create(&ctx.db, &[88u8; 32], "v".into(), &[0u8; 60]).await?;
        let user_id = 1;
        let token_a = Session::create(&ctx.db, user_id, &ctx.secret).await?;
        let token_b = Session::create(&ctx.db, user_id, &ctx.secret).await?;
        let auth_a = format!("Bearer {token_a}");
        let auth_b = format!("Bearer {token_b}");

        let open_endpoint = format!("{}/api/auth/mailbox/open", ctx.base_url);

        pace_auth().await;
        let pow = get_pow_header(&ctx.client, &ctx.base_url).await?;
        pace_auth().await;
        let open_resp = ctx
            .client
            .post(&open_endpoint)
            .header("X-Hashcash", &pow)
            .json(&json!({ "pub_temp": "aa".repeat(32) }))
            .send()
            .await?;
        assert_eq!(open_resp.status(), StatusCode::OK);
        let open_body: Value = open_resp.json().await?;
        let mailbox_id = open_body["mailbox_id"].as_str().unwrap();
        let polling_token = open_body["polling_token"].as_str().unwrap();

        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{mailbox_id}/fulfill",
            ctx.base_url
        );
        let pickup_endpoint =
            format!("{}/api/auth/mailbox/{mailbox_id}/pickup", ctx.base_url);

        let payload_a = "11".repeat(60);
        let payload_b = "22".repeat(60);

        pace().await;
        let req1 = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth_a)
            .json(&json!({ "payload": payload_a }))
            .send();
        let req2 = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth_b)
            .json(&json!({ "payload": payload_b }))
            .send();

        let (res1, res2) = tokio::join!(req1, req2);
        let status1 = res1?.status();
        let status2 = res2?.status();

        let statuses = [status1, status2];
        assert!(
            statuses.contains(&StatusCode::NO_CONTENT),
            "One concurrent fulfill must succeed with 204 NO_CONTENT"
        );
        assert!(
            statuses.contains(&StatusCode::BAD_REQUEST),
            "The other concurrent fulfill must receive 400 BAD_REQUEST"
        );

        // Verification: Pickup retrieves the payload that won the race
        pace_auth().await;
        let pickup_resp = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", format!("Bearer {polling_token}"))
            .send()
            .await?;
        assert_eq!(pickup_resp.status(), StatusCode::OK);

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_mailbox_uppercase_hex_payload_e2e() -> anyhow::Result<()> {
    with_sandbox_env("webdav_mailbox_uppercase_test", async move |ctx| {
        User::create(&ctx.db, &[66u8; 32], "v".into(), &[0u8; 60]).await?;
        let token = Session::create(&ctx.db, 1, &ctx.secret).await?;
        let auth = format!("Bearer {token}");

        let open_endpoint = format!("{}/api/auth/mailbox/open", ctx.base_url);

        pace_auth().await;
        let pow = get_pow_header(&ctx.client, &ctx.base_url).await?;
        pace_auth().await;
        let open_resp = ctx
            .client
            .post(&open_endpoint)
            .header("X-Hashcash", &pow)
            .json(&json!({ "pub_temp": "AA".repeat(32) }))
            .send()
            .await?;
        assert_eq!(open_resp.status(), StatusCode::OK);
        let open_body: Value = open_resp.json().await?;
        let mailbox_id = open_body["mailbox_id"].as_str().unwrap();
        let polling_token = open_body["polling_token"].as_str().unwrap();

        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{mailbox_id}/fulfill",
            ctx.base_url
        );
        let pickup_endpoint =
            format!("{}/api/auth/mailbox/{mailbox_id}/pickup", ctx.base_url);

        // Fulfill with UPPERCASE hexadecimal string (120 chars)
        let upper_payload = "DEADBEEF".repeat(15);
        assert_eq!(upper_payload.len(), 120);

        pace().await;
        let fulfill_resp = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth)
            .json(&json!({ "payload": upper_payload }))
            .send()
            .await?;
        assert_eq!(fulfill_resp.status(), StatusCode::NO_CONTENT);

        // Pickup returns normalized lowercase hexadecimal string
        pace_auth().await;
        let pickup_resp = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", format!("Bearer {polling_token}"))
            .send()
            .await?;
        assert_eq!(pickup_resp.status(), StatusCode::OK);
        let pickup_body: Value = pickup_resp.json().await?;
        assert_eq!(
            pickup_body,
            json!({
                "status": "fulfilled",
                "payload": "deadbeef".repeat(15)
            })
        );

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_mailbox_tampered_token_returns_unauthorized_e2e()
-> anyhow::Result<()> {
    with_sandbox_env("webdav_mailbox_tampered_token", async move |ctx| {
        User::create(&ctx.db, &[33u8; 32], "v".into(), &[0u8; 60]).await?;
        let valid_token = Session::create(&ctx.db, 1, &ctx.secret).await?;

        // Tamper with the MAC portion at the end of the session token
        let mut tampered_token = valid_token.clone();
        let last_char = tampered_token.pop().unwrap();
        let replacement = if last_char == '0' { '1' } else { '0' };
        tampered_token.push(replacement);

        let tampered_auth = format!("Bearer {tampered_token}");

        let offer_endpoint = format!("{}/api/v1/devices/mailbox", ctx.base_url);
        let link_ticket_endpoint =
            format!("{}/api/v1/devices/link-ticket", ctx.base_url);
        let status_endpoint =
            format!("{}/api/v1/devices/mailbox/01234567/status", ctx.base_url);
        let fulfill_endpoint =
            format!("{}/api/v1/devices/mailbox/01234567/fulfill", ctx.base_url);

        let r1 = ctx
            .client
            .post(&offer_endpoint)
            .header("Authorization", &tampered_auth)
            .send()
            .await?;
        assert_eq!(r1.status(), StatusCode::UNAUTHORIZED);

        let r2 = ctx
            .client
            .post(&link_ticket_endpoint)
            .header("Authorization", &tampered_auth)
            .send()
            .await?;
        assert_eq!(r2.status(), StatusCode::UNAUTHORIZED);

        let r3 = ctx
            .client
            .get(&status_endpoint)
            .header("Authorization", &tampered_auth)
            .send()
            .await?;
        assert_eq!(r3.status(), StatusCode::UNAUTHORIZED);

        let r4 = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &tampered_auth)
            .json(&json!({ "payload": "aa".repeat(60) }))
            .send()
            .await?;
        assert_eq!(r4.status(), StatusCode::UNAUTHORIZED);

        Ok(())
    })
    .await
}
