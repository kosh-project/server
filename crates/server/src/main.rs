// #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::net::{Ipv4Addr, SocketAddr};
use tokio::io;
use tokio::{
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
    storage::ledger,
};

use sqlx::sqlite::SqlitePoolOptions;

async fn shutdown_signal() -> io::Result<()> {
    type Err = io::Error;
    let ctrl_c = signal::ctrl_c();

    #[cfg(unix)]
    let term = async {
        use tokio::signal::unix::{SignalKind, signal};

        let mut signal = signal(SignalKind::terminate())?;

        signal.recv().await;

        Ok::<(), Err>(())
    };

    #[cfg(not(unix))]
    let term = async {
        std::future::pending::<()>().await;

        Ok::<(), Err>(())
    };

    tokio::select! {
        x = ctrl_c => { x?; },
        x = term => { x?; },
    }

    Ok(())
}

const PORT: u16 = 6969;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tokio::fs::create_dir_all("./test/vault").await?;
    info!(Module::Storage, "Vault initialized");

    #[allow(clippy::expect_used)]
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite://test/vault/metadata.db")
        .await?;

    let (log_sender, logger_handle) = logger::Service::start(1000)
        .await
        .map_err(|e| format!("Logging engine failed to boot {e}"))?;

    GLOBAL_LOGGER
        .set(log_sender)
        .map_err(|e| format!("Failed to initiate global logger {e:?}"))?;

    let app_state = AppStateBuilder::new()
        .db(pool.clone())
        .vault_path(std::path::PathBuf::from("./vault"))
        .build();

    let ledger_sender = app_state.ledger.sender().clone();

    let app = route_main(app_state);

    let addr = Ipv4Addr::from_octets([0, 0, 0, 0]);
    let listener = TcpListener::bind((addr, PORT)).await?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
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
                fatal!(Module::Server, "Server error {e}");
            }
        },
        res = shutdown_signal() => {
            res?;

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

    ledger::Handle::shutdown(&ledger_sender).await;

    shutdown!("Waiting to flush remaining entries...");
    logger_handle.shutdown_with_grace(10).await;

    eprintln!("Bye bye");
    Ok(())
}
