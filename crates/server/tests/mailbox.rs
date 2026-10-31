#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

mod boilerplate;
use boilerplate::with_sandbox_env;
use hyper::StatusCode;
use serde_json::{Value, json};
use server::model::{session::Session, user::User};

async fn pace() {
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_mailbox_auth_guard_and_unauthorized_access_e2e()
-> anyhow::Result<()> {
    with_sandbox_env("webdav_mailbox_auth_guard_test", async move |ctx| {
        let create_endpoint =
            format!("{}/api/v1/devices/mailbox", ctx.base_url);
        let fulfill_endpoint =
            format!("{}/api/v1/devices/mailbox/01234567/fulfill", ctx.base_url);
        let pickup_endpoint =
            format!("{}/api/v1/devices/mailbox/01234567", ctx.base_url);

        let valid_payload = json!({
            "payload": "ab".repeat(60)
        });

        // 1. Missing Authorization header returns 401 Unauthorized
        let r1 = ctx.client.post(&create_endpoint).send().await?;
        assert_eq!(r1.status(), StatusCode::UNAUTHORIZED);

        let r2 = ctx
            .client
            .post(&fulfill_endpoint)
            .json(&valid_payload)
            .send()
            .await?;
        assert_eq!(r2.status(), StatusCode::UNAUTHORIZED);

        let r3 = ctx.client.get(&pickup_endpoint).send().await?;
        assert_eq!(r3.status(), StatusCode::UNAUTHORIZED);

        // 2. Invalid / forged Bearer token returns 401 Unauthorized
        let forged_header = "Bearer forged_invalid_session_token";

        let r4 = ctx
            .client
            .post(&create_endpoint)
            .header("Authorization", forged_header)
            .send()
            .await?;
        assert_eq!(r4.status(), StatusCode::UNAUTHORIZED);

        let r5 = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", forged_header)
            .json(&valid_payload)
            .send()
            .await?;
        assert_eq!(r5.status(), StatusCode::UNAUTHORIZED);

        let r6 = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", forged_header)
            .send()
            .await?;
        assert_eq!(r6.status(), StatusCode::UNAUTHORIZED);

        Ok(())
    })
    .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_mailbox_full_lifecycle_and_burn_after_reading_e2e()
-> anyhow::Result<()> {
    with_sandbox_env("webdav_mailbox_lifecycle_test", async move |ctx| {
        // Create user 1 with two active device sessions: Receiver and Sender
        User::create(&ctx.db, &[1u8; 32], "test_verifier".into(), &[0u8; 60])
            .await?;
        let user_id = 1;
        let receiver_token =
            Session::create(&ctx.db, user_id, &ctx.secret).await?;
        let sender_token =
            Session::create(&ctx.db, user_id, &ctx.secret).await?;

        let receiver_auth = format!("Bearer {receiver_token}");
        let sender_auth = format!("Bearer {sender_token}");

        let create_endpoint =
            format!("{}/api/v1/devices/mailbox", ctx.base_url);

        // 1. Receiver creates mailbox
        let create_resp = ctx
            .client
            .post(&create_endpoint)
            .header("Authorization", &receiver_auth)
            .send()
            .await?;
        assert_eq!(create_resp.status(), StatusCode::OK);

        let create_body: Value = create_resp.json().await?;
        let mailbox_id = create_body["mailbox_id"]
            .as_str()
            .expect("mailbox_id is string");
        assert_eq!(mailbox_id.len(), 8, "Mailbox ID must be 8 chars");
        assert_eq!(create_body["expires_in"], 300);

        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{mailbox_id}/fulfill",
            ctx.base_url
        );
        let pickup_endpoint =
            format!("{}/api/v1/devices/mailbox/{mailbox_id}", ctx.base_url);

        // 2. Receiver polls pending mailbox before fulfillment
        pace().await;
        let poll_resp = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &receiver_auth)
            .send()
            .await?;
        assert_eq!(poll_resp.status(), StatusCode::OK);
        let poll_body: Value = poll_resp.json().await?;
        assert_eq!(poll_body, json!({ "status": "pending" }));

        // 3. Subsequent poll while still pending is non-destructive
        pace().await;
        let poll_resp2 = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &receiver_auth)
            .send()
            .await?;
        assert_eq!(poll_resp2.status(), StatusCode::OK);
        let poll_body2: Value = poll_resp2.json().await?;
        assert_eq!(poll_body2, json!({ "status": "pending" }));

        // 4. Sender fulfills mailbox with valid hex payload (128 hex chars / 64 bytes)
        let hex_payload = "cafe".repeat(32);
        assert_eq!(hex_payload.len(), 128);

        pace().await;
        let fulfill_resp = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &sender_auth)
            .json(&json!({ "payload": hex_payload }))
            .send()
            .await?;
        assert_eq!(fulfill_resp.status(), StatusCode::NO_CONTENT);

        // 5. Double fulfill rejection: fulfilling again returns 400 Bad Request
        pace().await;
        let double_fulfill_resp = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &sender_auth)
            .json(&json!({ "payload": hex_payload }))
            .send()
            .await?;
        assert_eq!(double_fulfill_resp.status(), StatusCode::BAD_REQUEST);

        // 6. Receiver picks up fulfilled payload
        pace().await;
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
                "payload": hex_payload
            })
        );

        // 7. Single-use burn-after-reading: polling again returns 404 Not Found
        pace().await;
        let burn_resp = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &receiver_auth)
            .send()
            .await?;
        assert_eq!(burn_resp.status(), StatusCode::NOT_FOUND);

        // 8. Fulfill after consumption returns 404 Not Found
        pace().await;
        let post_burn_fulfill = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &sender_auth)
            .json(&json!({ "payload": hex_payload }))
            .send()
            .await?;
        assert_eq!(post_burn_fulfill.status(), StatusCode::NOT_FOUND);

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

        let create_endpoint =
            format!("{}/api/v1/devices/mailbox", ctx.base_url);

        // User 1 creates mailbox
        let create_resp = ctx
            .client
            .post(&create_endpoint)
            .header("Authorization", &u1_auth)
            .send()
            .await?;
        assert_eq!(create_resp.status(), StatusCode::OK);
        let body: Value = create_resp.json().await?;
        let u1_mailbox_id = body["mailbox_id"].as_str().unwrap();

        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{u1_mailbox_id}/fulfill",
            ctx.base_url
        );
        let pickup_endpoint =
            format!("{}/api/v1/devices/mailbox/{u1_mailbox_id}", ctx.base_url);

        let valid_payload = json!({
            "payload": "1234".repeat(30) // 120 hex chars
        });

        // 1. User 2 attempts to poll User 1's pending mailbox -> 404 Not Found
        pace().await;
        let u2_poll = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &u2_auth)
            .send()
            .await?;
        assert_eq!(u2_poll.status(), StatusCode::NOT_FOUND);

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

        // 3. User 1's mailbox was untouched: User 1 polls and still sees Pending
        pace().await;
        let u1_poll = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &u1_auth)
            .send()
            .await?;
        assert_eq!(u1_poll.status(), StatusCode::OK);
        let u1_poll_body: Value = u1_poll.json().await?;
        assert_eq!(u1_poll_body, json!({ "status": "pending" }));

        // 4. User 1 fulfills their mailbox -> 204 No Content
        pace().await;
        let u1_fulfill = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &u1_auth)
            .json(&valid_payload)
            .send()
            .await?;
        assert_eq!(u1_fulfill.status(), StatusCode::NO_CONTENT);

        // 5. User 2 attempts to pickup User 1's fulfilled mailbox -> 404 Not Found
        pace().await;
        let u2_pickup = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &u2_auth)
            .send()
            .await?;
        assert_eq!(u2_pickup.status(), StatusCode::NOT_FOUND);

        // 6. User 1 picks up their payload successfully
        pace().await;
        let u1_pickup = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &u1_auth)
            .send()
            .await?;
        assert_eq!(u1_pickup.status(), StatusCode::OK);
        let u1_pickup_body: Value = u1_pickup.json().await?;
        assert_eq!(
            u1_pickup_body,
            json!({
                "status": "fulfilled",
                "payload": "1234".repeat(30)
            })
        );

        // 7. Mailbox is burned
        pace().await;
        let u1_post_burn = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &u1_auth)
            .send()
            .await?;
        assert_eq!(u1_post_burn.status(), StatusCode::NOT_FOUND);

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

        let create_endpoint =
            format!("{}/api/v1/devices/mailbox", ctx.base_url);

        let create_resp = ctx
            .client
            .post(&create_endpoint)
            .header("Authorization", &auth)
            .send()
            .await?;
        assert_eq!(create_resp.status(), StatusCode::OK);
        let body: Value = create_resp.json().await?;
        let mailbox_id = body["mailbox_id"].as_str().unwrap();

        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{mailbox_id}/fulfill",
            ctx.base_url
        );
        let pickup_endpoint =
            format!("{}/api/v1/devices/mailbox/{mailbox_id}", ctx.base_url);

        // 1. Payload under min length (118 hex chars) -> 400 Bad Request
        pace().await;
        let too_short = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth)
            .json(&json!({ "payload": "aa".repeat(59) }))
            .send()
            .await?;
        assert_eq!(too_short.status(), StatusCode::BAD_REQUEST);

        // 2. Payload over max length (8194 hex chars) -> 400 Bad Request
        pace().await;
        let too_long = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth)
            .json(&json!({ "payload": "bb".repeat(4097) }))
            .send()
            .await?;
        assert_eq!(too_long.status(), StatusCode::BAD_REQUEST);

        // 3. Odd length hex characters (121 chars) -> 400 Bad Request
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

        // 4. Invalid hex characters (120 chars with 'zz') -> 400 Bad Request
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

        // 5. Empty payload -> 400 Bad Request
        pace().await;
        let empty_resp = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth2)
            .json(&json!({ "payload": "" }))
            .send()
            .await?;
        assert_eq!(empty_resp.status(), StatusCode::BAD_REQUEST);

        // 6. Exactly min boundary length (120 hex chars / 60 bytes) -> succeeds (204)
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

        // 7. Pickup retrieves exact payload
        pace().await;
        let pickup_resp = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &auth2)
            .send()
            .await?;
        assert_eq!(pickup_resp.status(), StatusCode::OK);
        let pickup_body: Value = pickup_resp.json().await?;
        assert_eq!(
            pickup_body,
            json!({
                "status": "fulfilled",
                "payload": exact_min
            })
        );

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

        let nonexistent_pickup =
            format!("{}/api/v1/devices/mailbox/00000000", ctx.base_url);
        let nonexistent_fulfill =
            format!("{}/api/v1/devices/mailbox/00000000/fulfill", ctx.base_url);

        let missing_get = ctx
            .client
            .get(&nonexistent_pickup)
            .header("Authorization", &auth)
            .send()
            .await?;
        assert_eq!(missing_get.status(), StatusCode::NOT_FOUND);

        pace().await;
        let missing_post = ctx
            .client
            .post(&nonexistent_fulfill)
            .header("Authorization", &auth)
            .json(&json!({ "payload": "ff".repeat(60) }))
            .send()
            .await?;
        assert_eq!(missing_post.status(), StatusCode::NOT_FOUND);

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

        let create_endpoint =
            format!("{}/api/v1/devices/mailbox", ctx.base_url);

        let create_resp = ctx
            .client
            .post(&create_endpoint)
            .header("Authorization", &auth)
            .send()
            .await?;
        assert_eq!(create_resp.status(), StatusCode::OK);
        let body: Value = create_resp.json().await?;
        let mailbox_id = body["mailbox_id"].as_str().unwrap();

        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{mailbox_id}/fulfill",
            ctx.base_url
        );
        let pickup_endpoint =
            format!("{}/api/v1/devices/mailbox/{mailbox_id}", ctx.base_url);

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

        pace().await;
        let pickup_resp = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &auth)
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
        let token_a = Session::create(&ctx.db, user_id, &ctx.secret).await?;
        let token_b = Session::create(&ctx.db, user_id, &ctx.secret).await?;
        let auth_a = format!("Bearer {token_a}");
        let auth_b = format!("Bearer {token_b}");

        let create_endpoint =
            format!("{}/api/v1/devices/mailbox", ctx.base_url);
        let create_resp = ctx
            .client
            .post(&create_endpoint)
            .header("Authorization", &auth_a)
            .send()
            .await?;
        assert_eq!(create_resp.status(), StatusCode::OK);
        let body: Value = create_resp.json().await?;
        let mailbox_id = body["mailbox_id"].as_str().unwrap();

        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{mailbox_id}/fulfill",
            ctx.base_url
        );
        let pickup_endpoint =
            format!("{}/api/v1/devices/mailbox/{mailbox_id}", ctx.base_url);

        let payload = "42".repeat(60);
        pace().await;
        let fulfill_resp = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &auth_a)
            .json(&json!({ "payload": payload }))
            .send()
            .await?;
        assert_eq!(fulfill_resp.status(), StatusCode::NO_CONTENT);

        // Concurrently attempt pickup from two distinct sessions
        pace().await;
        let req1 = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &auth_a)
            .send();
        let req2 = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &auth_b)
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

        let create_endpoint =
            format!("{}/api/v1/devices/mailbox", ctx.base_url);
        let create_resp = ctx
            .client
            .post(&create_endpoint)
            .header("Authorization", &auth_a)
            .send()
            .await?;
        assert_eq!(create_resp.status(), StatusCode::OK);
        let body: Value = create_resp.json().await?;
        let mailbox_id = body["mailbox_id"].as_str().unwrap();

        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{mailbox_id}/fulfill",
            ctx.base_url
        );
        let pickup_endpoint =
            format!("{}/api/v1/devices/mailbox/{mailbox_id}", ctx.base_url);

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
        pace().await;
        let pickup_resp = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &auth_a)
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

        let create_endpoint =
            format!("{}/api/v1/devices/mailbox", ctx.base_url);
        let create_resp = ctx
            .client
            .post(&create_endpoint)
            .header("Authorization", &auth)
            .send()
            .await?;
        assert_eq!(create_resp.status(), StatusCode::OK);
        let body: Value = create_resp.json().await?;
        let mailbox_id = body["mailbox_id"].as_str().unwrap();

        let fulfill_endpoint = format!(
            "{}/api/v1/devices/mailbox/{mailbox_id}/fulfill",
            ctx.base_url
        );
        let pickup_endpoint =
            format!("{}/api/v1/devices/mailbox/{mailbox_id}", ctx.base_url);

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
        pace().await;
        let pickup_resp = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &auth)
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

        // Session token format: {session_id}.{expires_at}.{blake3_mac}
        // Tamper with the MAC portion at the end
        let mut tampered_token = valid_token.clone();
        let last_char = tampered_token.pop().unwrap();
        let replacement = if last_char == '0' { '1' } else { '0' };
        tampered_token.push(replacement);

        let tampered_auth = format!("Bearer {tampered_token}");

        let create_endpoint =
            format!("{}/api/v1/devices/mailbox", ctx.base_url);
        let resp = ctx
            .client
            .post(&create_endpoint)
            .header("Authorization", &tampered_auth)
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        pace().await;
        let fulfill_endpoint =
            format!("{}/api/v1/devices/mailbox/01234567/fulfill", ctx.base_url);
        let resp_fulfill = ctx
            .client
            .post(&fulfill_endpoint)
            .header("Authorization", &tampered_auth)
            .json(&json!({ "payload": "aa".repeat(60) }))
            .send()
            .await?;
        assert_eq!(resp_fulfill.status(), StatusCode::UNAUTHORIZED);

        pace().await;
        let pickup_endpoint =
            format!("{}/api/v1/devices/mailbox/01234567", ctx.base_url);
        let resp_pickup = ctx
            .client
            .get(&pickup_endpoint)
            .header("Authorization", &tampered_auth)
            .send()
            .await?;
        assert_eq!(resp_pickup.status(), StatusCode::UNAUTHORIZED);

        Ok(())
    })
    .await
}
