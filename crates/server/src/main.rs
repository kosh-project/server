use std::net::Ipv4Addr;

use tokio::{
    io::{self},
    net::TcpListener,
    pin, signal,
    sync::watch,
    time::{Duration, timeout},
};
use webdav_server::{
    api::route::route_main,
    app::AppStateBuilder,
    error, fatal, info,
    logger::{self, GLOBAL_LOGGER, Module},
    shutdown,
};

use sqlx::sqlite::SqlitePoolOptions;

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler")
    };

    #[cfg(unix)]
    let term = async {
        use tokio::signal::unix::{SignalKind, signal};

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

const PORT: u16 = 6969;

#[tokio::main]
async fn main() -> io::Result<()> {
    tokio::fs::create_dir_all("./test/vault").await?;
    info!(Module::Storage, "Vault initialized");

    #[allow(clippy::expect_used)]
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite://test/vault/metadata.db")
        .await
        .expect("Failed to connect to SQLite!");

    let (log_sender, logger_handle) =
        logger::Service::start(1000).await.unwrap_or_else(|e| {
            panic!("Logging engine failed to boot {e}")
        });

    GLOBAL_LOGGER
        .set(log_sender)
        .expect("Failed to initiate global logger");

    let app_state = AppStateBuilder::new()
        .db(pool.clone())
        .vault_path(std::path::PathBuf::from("./vault"))
        .build();

    let app = route_main(app_state);

    let addr = Ipv4Addr::new(0, 0, 0, 0);
    let listener = TcpListener::bind((addr, PORT)).await?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let mut rx = shutdown_rx.clone();
            let _ = rx.changed().await;

            info!(Module::Server, "Graceful shutdown initiated...");
        })
        .into_future();

    pin!(server);
    info!(Module::Server, "Listening on port {PORT}");

    tokio::select! {
        res = &mut server => {
            if let Err(e) = res {
                fatal!(Module::Server, "Server error {e}")
            }
        },
        () = shutdown_signal() => {
            info!(Module::Server, "Shutdown signal recieved. Ignoring any new connections...");

            let _ = shutdown_tx.send(true);

            match timeout(Duration::from_secs(10), &mut server).await {
                Ok(_) => info!(Module::Server, "All active connections closed successfully."),
                Err(_) => error!(Module::Server, "Grace period expired. Forcefully killing lingering connections."),
            }
        }

    };

    info!(
        Module::Database,
        "Safely closing database connection pool..."
    );
    pool.close().await;

    shutdown!("Waiting to flush remaining entries...");
    logger_handle.shutdown_with_grace(10).await;

    eprintln!("Bye bye");
    Ok(())
}
