use boot::Error;
use kosh_core::config::Config;
use kosh_core::tls;
use rustls::crypto::ring;
use std::str::FromStr;
use tokio::io;
use tokio::{signal, sync::watch};
use webdav_server::error::boot;
use webdav_server::server::Launcher;
use webdav_server::{
    api::route::route_main,
    app::AppStateBuilder,
    info,
    logger::{self, GLOBAL_LOGGER, Module},
    shutdown,
    storage::ledger,
};

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions,
    SqliteSynchronous,
};

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

#[tokio::main]
async fn main() -> miette::Result<()> {
    boot().await?;

    Ok(())
}

async fn boot() -> Result<(), boot::Error> {
    let _ = ring::default_provider().install_default();

    let config = Config::load_or_init().await?;

    tokio::fs::create_dir_all(&config.vault_path).await?;
    info!(
        Module::Storage,
        "Vault initialized at {}",
        config.vault_path.display()
    );

    let db_path =
        format!("sqlite://{}/metadata.db", config.vault_path.display());

    let options = SqliteConnectOptions::from_str(&db_path)?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .pragma("mmap_size", "30000000000")
        .pragma("temp_store", "MEMORY")
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(50)
        .connect_with(options)
        .await?;

    let (log_sender, logger_handle) = logger::Service::start(&config)
        .await
        .map_err(|e| boot::Error::Logger(e.to_string()))?;

    GLOBAL_LOGGER.set(log_sender).map_err(|e| {
        Error::Logger(format!("Failed to initiate global logger {e:?}"))
    })?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    info!(Module::Database, "Applied all pending SQLite migrations");

    let app_state = AppStateBuilder::new()
        .db(pool.clone())
        .vault_path(config.vault_path.clone())
        .build();

    let identity =
        tls::Identity::load_or_create(app_state.vault_path().to_owned())
            .await?;

    let ledger_sender = app_state.ledger.sender().clone();
    let app = route_main(app_state);

    let (shutdown_tx, _rx) = watch::channel(false);

    let launcher = Launcher::new(
        config.port,
        app,
        identity,
        shutdown_tx,
        config.enable_tls,
    );
    launcher.run(shutdown_signal()).await?;

    info!(
        Module::Database,
        "Safely closing database connection pool..."
    );
    pool.close().await;

    ledger::Handle::shutdown(&ledger_sender).await;

    shutdown!("Waiting to flush remaining entries...");
    logger_handle.shutdown_with_grace(10).await;

    Ok(())
}
