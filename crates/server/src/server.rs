use std::{net::SocketAddr, time::Duration};

use axum::Router;
use axum_server::{Handle, tls_rustls::RustlsConfig};
use kosh_core::{logger::Module, tls};
use tokio::{io, sync::watch};

use crate::{error::boot, info};

pub struct Launcher {
    port: u16,
    app: Router,
    identity: tls::Identity,
    shutdown_tx: watch::Sender<bool>,
    enable_tls: bool,
}

impl Launcher {
    #[must_use]
    pub const fn new(
        port: u16,
        app: Router,
        identity: tls::Identity,
        shutdown_tx: watch::Sender<bool>,
        enable_tls: bool,
    ) -> Self {
        Self {
            port,
            app,
            identity,
            shutdown_tx,
            enable_tls,
        }
    }

    pub async fn run<F>(self, shutdown_signal: F) -> Result<(), boot::Error>
    where
        F: Future<Output = io::Result<()>> + Send + 'static,
    {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));

        let handle = Handle::new();
        let shutdown_handle = handle.clone();
        let tx = self.shutdown_tx;

        tokio::spawn(async move {
            let _ = shutdown_signal.await;
            info!(
                Module::Server,
                "Shutdown signal received. Refusing new connections..."
            );
            let _ = tx.send(true);
            shutdown_handle.graceful_shutdown(Some(Duration::from_secs(10)));
        });

        if self.enable_tls {
            info!(
                Module::Server,
                "Starting server in SECURE TLS mode on port {}", self.port
            );
            let config = RustlsConfig::from_pem(
                self.identity.cert_pem.into_bytes(),
                self.identity.key_pem.into_bytes(),
            )
            .await?;

            axum_server::bind_rustls(addr, config)
                .handle(handle)
                .serve(
                    self.app
                        .into_make_service_with_connect_info::<SocketAddr>(),
                )
                .await?;
        } else {
            info!(
                Module::Server,
                "Starting server in PLAIN_TEXT mode on port {}", self.port
            );

            axum_server::bind(addr)
                .handle(handle)
                .serve(
                    self.app
                        .into_make_service_with_connect_info::<SocketAddr>(),
                )
                .await?;
        }

        Ok(())
    }
}
