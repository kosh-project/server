use tokio::{
    io::{self},
    net::TcpListener,
    pin, signal,
    sync::watch,
    time::{Duration, timeout},
};
use webdav_server::{
    api::route::route_main, app::AppStateBuilder, log,
};

use sqlx::sqlite::SqlitePoolOptions; // You need this import!

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler")
    };

    #[cfg(unix)]
    let term = async {
        use tokio::signal::unix::{self, SignalKind, signal};
        signal(SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await
    };

    #[cfg(not(unix))]
    let term = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    tokio::fs::create_dir_all("./test/vault").await?;
    log!("FS", "Initialized vault");

    #[allow(clippy::expect_used)]
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite://test/vault/metadata.db")
        .await
        .expect("Failed to connect to SQLite!");

    let app_state = AppStateBuilder::new()
        .db(pool.clone())
        .vault_path(std::path::PathBuf::from("./vault"))
        .build();

    let app = route_main(app_state);

    let listener = TcpListener::bind("0.0.0.0:6969").await?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let mut rx = shutdown_rx.clone();
            let _ = rx.changed().await;

            log!("SERVER", "Graceful shutdown initiated...");
        })
        .into_future();

    pin!(server);
    log!("SERVER", "Listening on port 6969");

    tokio::select! {
        res = &mut server => {
            if let Err(e) = res {
                log!("SERVER", format!("Server error: {e}"))
            }
        },
        _ = shutdown_signal() => {
            log!("SERVER", "Shutdown signal recieved. Ignoring any new connections...");

            let _ = shutdown_tx.send(true);

            match timeout(Duration::from_secs(10), &mut server).await {
                Ok(_) => log!("SERVER", "All active connections closed successfully."),
                Err(_) => log!("SERVER", "Grace period expired. Forcefully killing lingering connections."),
            }
        }

    };
    log!("DB", "Safely closing database connection pool...");
    pool.close().await;
    log!("SERVER", "Bye bye");
    Ok(())
}
