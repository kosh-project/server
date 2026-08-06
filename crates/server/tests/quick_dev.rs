use sqlx::sqlite::SqlitePoolOptions;
use tmpdir::TmpDir;
use tokio::net::TcpListener;
use webdav_server::{api::route::route_main, app::AppStateBuilder};

#[tokio::test]
#[serial_test::serial]
async fn quick_dev() -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let sql_pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite::memory:")
        .await?;

    let state = AppStateBuilder::new()
        .vault_path(TmpDir::new("web-dav_vault").await?.to_path_buf())
        .db(sql_pool)
        .build();

    tokio::spawn(async move {
        let _ = axum::serve(listener, route_main(state)).await;
    });

    let client = httpc_test::new_client(format!("http://{addr}"))?;

    client.do_get("/health").await?.print().await?;
    client.do_get("/storage").await?.print().await?;

    Ok(())
}
