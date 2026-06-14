use hyper::StatusCode;
use reqwest::multipart;
use tmpdir::TmpDir;
use tokio::{fs, net::TcpListener};
use webdav_server::{api::route::router, state::AppStateBuilder};


#[tokio::test]
#[serial_test::serial]
async fn test_multipart_upload_integrity() -> anyhow::Result<()> {
    let _tmp = TmpDir::new("webdav").await?;
    let vault_dir = _tmp.to_path_buf();

    tokio::fs::create_dir_all(&vault_dir).await?;

    let file_name = "lmao_dead_ok.enc";
    let file_path = vault_dir.join(file_name);

    let _ = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&file_path)
        .await?;

    let fake_encrypted_payload = (0..1_000_000)
        .map(|_| rand::random::<u8>())
        .collect::<Vec<u8>>();

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let state = AppStateBuilder::new().vault_path(vault_dir).build();

    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });

    let file_part = multipart::Part::bytes(fake_encrypted_payload.clone())
        .file_name(file_name)
        .mime_str("application/octet-stream")?;

    let form = multipart::Form::new().part("file", file_part);

    let client = reqwest::Client::new();

    let response = client
        .post(format!("http://{addr}/upload"))
        .multipart(form)
        .send()
        .await?;

    assert_eq!(response.status(), StatusCode::OK, "Server Operation failed",);

    // eprintln!("Fails??");
    let written_bytes = fs::read(file_path).await?;

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
