// #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::handler::HandlerWithoutStateExt;
use axum_server::tls_rustls::RustlsConfig;
use bincode_next::fingerprint;
use miette::IntoDiagnostic;
use miette::miette;
use std::error::Error;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use tokio::io;
use tokio::{
    net::TcpListener,
    pin, signal,
    sync::watch,
    time::{Duration, timeout},
};
use webdav_server::config::config::Config;
use webdav_server::error::boot;
use webdav_server::server::Launcher;
use webdav_server::tls;
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
async fn main() -> miette::Result<()> {
    boot().await?;

    Ok(())
}

async fn boot() -> Result<(), boot::Error> {
    let config_path = Path::new("test/kosh.kdl");
    let config = Config::load_or_init(config_path).await?;

    tokio::fs::create_dir_all(&config.vault_path).await?;
    info!(
        Module::Storage,
        "Vault initialized at {}",
        config.vault_path.display()
    );

    let db_path =
        format!("sqlite://{}/metadata.db", config.vault_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_path)
        .await?;

    let (log_sender, logger_handle) = logger::Service::start(1000)
        .await
        .map_err(|e| boot::Error::Logger(e.to_string()))?;

    GLOBAL_LOGGER.set(log_sender).map_err(|e| {
        boot::Error::Logger(format!("Failed to initiate global logger {e:?}"))
    })?;

    let app_state = AppStateBuilder::new()
        .db(pool.clone())
        .vault_path(config.vault_path.clone())
        .build();

    let identity =
        tls::Identity::load_or_create(app_state.vault_path().to_owned())
            .await?;

    let fingerprint = identity.fingerprint()?;

    info!(Module::Server, "TLS Fingerprint : {}", fingerprint);

    let ledger_sender = app_state.ledger.sender().clone();
    let app = route_main(app_state);

    let (shutdown_tx, _rx) = watch::channel(false);

    let launcher = Launcher::new(config.port, app, identity, shutdown_tx);
    launcher.run(shutdown_signal()).await?;

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
