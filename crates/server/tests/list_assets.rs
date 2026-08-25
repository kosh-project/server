mod boilerplate;
use boilerplate::with_sandbox_env;
use hyper::StatusCode;
use serde_json::Value;
use uuid::Uuid;
use webdav_server::model::{session::Session, user::User};

#[tokio::test]
#[serial_test::serial]
async fn test_asset_listing_endpoint_e2e() -> anyhow::Result<()> {
    with_sandbox_env("webdav_list_test", async move |ctx| {
        User::create(&ctx.db, &vec![0u8; 32], "fake_verifier".into()).await?;
        let user_id = 1;

        let token = Session::create(&ctx.db, user_id).await?;

        let endpoint = format!("{}/api/v1/assets", &ctx.base_url);

        // Test: Unauthenticated Request
        let fail_resp = ctx.client.get(&endpoint).send().await?;
        assert_eq!(
            fail_resp.status(),
            StatusCode::UNAUTHORIZED,
            "Expected 401 for missing token"
        );

        // Test: Authenticated Reuest (Zero Assts)
        let empty_resp = ctx
            .client
            .get(&endpoint)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?;

        assert_eq!(empty_resp.status(), StatusCode::OK);

        let empty_list: Value = empty_resp.json().await?;
        assert!(
            empty_list["assets"].as_array().unwrap().is_empty(),
            "Expected 0 assets in list"
        );

        // Test: Authenticated Request (but with Assets)
        sqlx::query!(r#"
            INSERT INTO assets (id, user_id, hash, size_bytes, last_modified, tag)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            Uuid::new_v4().as_bytes().to_vec(),
            user_id,
            b"very real hash" as &[u8],
            1024,
            99_999,
            "0"
        ).execute(&ctx.db).await?;

        let resp = ctx.client.get(format!("{}?tag=0", endpoint))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?;

        assert_eq!(resp.status(), StatusCode::OK);

        let asset_json : Value = resp.json().await?;
        let assets = asset_json["assets"].as_array().unwrap();

        assert_eq!(assets.len(), 1, "Expect exactly 1 asset");
        assert_eq!(assets[0]["size_bytes"], 1024);
        assert_eq!(assets[0]["tag"], 0);

        assert_eq!(assets[0]["hash"], hex::encode(b"very real hash"));

        Ok(())
    })
    .await?;

    Ok(())
}
