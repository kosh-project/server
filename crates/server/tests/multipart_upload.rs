use hyper::StatusCode;
use sqlx::sqlite::SqlitePoolOptions;
use tmpdir::TmpDir;
use tokio::{fs, net::TcpListener};
use webdav_server::{
    api::route::route_main,
    app::AppStateBuilder,
    model::{session::Session, user::User},
};

#[tokio::test]
#[serial_test::serial]
async fn test_multipart_upload_integrity() -> anyhow::Result<()> {
    let _tmp = TmpDir::new("webdav").await?;
    let vault_dir = _tmp.to_path_buf();

    tokio::fs::create_dir_all(&vault_dir).await?;

    let file_name = "lmao_dead_ok.enc";

    let fake_encrypted_payload = (0..1_000_000)
        .map(|_| rand::random::<u8>())
        .collect::<Vec<u8>>();

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let sql_pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite::memory:")
        .await?;

    sqlx::migrate!("./migrations").run(&sql_pool).await?;

    User::create(&sql_pool, &vec![0; 32], "fake_verifier".into()).await?;

    let user_id = 1;
    let token = Session::create(&sql_pool, user_id).await?;

    let state = AppStateBuilder::new()
        .vault_path(&vault_dir)
        .db(sql_pool)
        .build();

    tokio::spawn(async move {
        axum::serve(listener, route_main(state)).await.unwrap()
    });

    let client = reqwest::Client::new();

    let response = client
        .post(format!("http://{addr}/api/v1/upload/0"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-File-Name", file_name)
        .header("Content-Length", fake_encrypted_payload.len().to_string())
        .body(fake_encrypted_payload.clone())
        .send()
        .await?;

    assert_eq!(response.status(), StatusCode::OK, "Server Operation failed",);

    let response_json: serde_json::Value = response.json().await?;

    let hash_str = response_json["hash"].as_str().unwrap();

    // eprintln!("Fails??");
    let written_bytes = fs::read(vault_dir.join(hash_str)).await?;

    assert_eq!(
        written_bytes.len(),
        fake_encrypted_payload.len(),
        "committed + uploaded payload length mismatch"
    );

    assert_eq!(
        written_bytes, fake_encrypted_payload,
        "uploaded payload != committed content"
    );

    Ok(())
}
