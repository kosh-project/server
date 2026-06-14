use tmpdir::TmpDir;
use tokio::net::TcpListener;
use webdav_server::{api::route::router, state::AppStateBuilder};

#[tokio::test]
#[serial_test::serial]
async fn quick_dev() -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let state = AppStateBuilder::new()
        .vault_path(TmpDir::new("web-dav_vault").await?.to_path_buf())
        .build();

    tokio::spawn(async move {
        let _ = axum::serve(listener, router(state)).await;
    });

    let client = httpc_test::new_client(format!("http://{addr}"))?;

    client.do_get("/health").await?.print().await?;
    client.do_get("/storage").await?.print().await?;

    Ok(())
}
