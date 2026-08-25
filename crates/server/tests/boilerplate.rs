use std::path::PathBuf;

use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use tmpdir::TmpDir;
use tokio::{fs::create_dir_all, net::TcpListener};
use webdav_server::{api::route::route_main, app::AppStateBuilder};

pub struct TestCtx {
    pub db: SqlitePool,
    pub vault_path: PathBuf,
    pub base_url: String,
    pub client: reqwest::Client,
}

pub async fn with_sandbox_env<F, Fut, P, T>(
    tmp_path: P,
    f: F,
) -> anyhow::Result<()>
where
    P: AsRef<str>,
    F: FnOnce(TestCtx) -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let _tmp = TmpDir::new(tmp_path).await?;
    let vault_dir = _tmp.to_path_buf();
    create_dir_all(&vault_dir).await?;

    let listener = TcpListener::bind("127.0.0.1:0").await?;

    let addr = listener.local_addr()?;
    let base_url = format!("http://{}", addr);

    let sql_pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite::memory:")
        .await?;

    sqlx::migrate!("./migrations").run(&sql_pool).await?;

    let state = AppStateBuilder::new()
        .vault_path(vault_dir.clone())
        .db(sql_pool.clone())
        .build();

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base_url = format!("http://{}", listener.local_addr()?);

    tokio::spawn(async move {
        axum::serve(listener, route_main(state)).await.unwrap();
    });

    let ctx = TestCtx {
        db: sql_pool,
        vault_path: vault_dir.clone(),
        base_url,
        client: reqwest::Client::new(),
    };

    f(ctx).await?;
    Ok(())
}
