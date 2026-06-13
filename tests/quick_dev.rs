use httpc_test::Result;
use tokio::net::TcpListener;
use webdav_server::router;

#[tokio::test]
#[serial_test::serial]
async fn quick_dev() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    tokio::spawn(async move {
        let _ = axum::serve(listener, router()).await;
    });

    let client = httpc_test::new_client(format!("http://{addr}"))?;

    client.do_get("/health").await?.print().await?;
    client.do_get("/storage").await?.print().await?;

    Ok(())
}
