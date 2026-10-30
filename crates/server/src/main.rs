use kosh_core::config::Config;
use kosh_core::tls;
use rustls::crypto::ring;
use server::auth::Secret;
use server::error::boot;
use server::error::boot::Error::AlreadyInitiated;
use server::mdns;
use server::model::database;
use server::server::Launcher;
use server::storage;
use server::{
    api::route::route_main,
    app::AppStateBuilder,
    info,
    logger::{self, Module},
    shutdown,
    storage::ledger,
};
use tokio::io;
use tokio::{signal, sync::watch};

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
    ring::default_provider()
        .install_default()
        .map_err(|_| AlreadyInitiated("Crypto Ring Provider"))?;

    let config = Config::load_or_init().await?;

    let logger_handle = logger::init(&config).await?;
    storage::init(&config).await?;
    let pool = database::init(&config).await?;
    let secret = Secret::load_or_create(&config).await?;
    let identity = tls::init(&config).await?;

    let app_state = AppStateBuilder::new()
        .db(pool.clone())
        .vault_path(config.vault_path.clone())
        .secret(secret)
        .build();

    let ledger_sender = app_state.ledger.sender().clone();
    let app = route_main(app_state);

    let broadcaster = mdns::init(&config);

    let (shutdown_tx, _rx) = watch::channel(false);

    let launcher = Launcher::new(
        config.port,
        app,
        identity,
        shutdown_tx,
        config.enable_tls,
    );
    launcher.run(shutdown_signal()).await?;

    if let Some(b) = broadcaster {
        b.shutdown();
        info!(Module::Server, "mDNS broadcaster shut down cleanly");
    }

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
