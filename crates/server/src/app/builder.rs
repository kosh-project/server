use std::{path::PathBuf, sync::Arc, time::Duration};

use moka::future::Cache;
use sqlx::SqlitePool;
use url::Url;
use webauthn_rs::{Webauthn, WebauthnBuilder};

use crate::{
    app::{Passkey, State as AppState, state::UserId},
    auth::Secret,
    model::session::TokenHash,
    storage::{self, ledger},
};

/// A builder for constructing [`AppState`].
///
/// This is the intended way to create an `AppState` both in `main.rs` and in tests.
/// Using a builder makes it easy to inject different databases and vault paths
/// in test environments without modifying the production code path.
///
/// # Example
///
/// ```rust,no_run
/// use webdav_server::app::AppStateBuilder;
///
/// # let pool = todo!(); // SqlitePool — provided at startup
/// let state = AppStateBuilder::new()
///     .vault_path("./vault")
///     .db(pool)
///     .build();
/// ```
///
/// [`AppState`]: crate::app::State
pub struct AppStateBuilder {
    vault_path: Option<PathBuf>,
    db: Option<SqlitePool>,
    session_cache: Option<Cache<TokenHash, UserId>>,
    secret: Option<Secret>,
    webauthn: Option<Arc<Webauthn>>,
}

impl AppStateBuilder {
    /// Creates a new builder with no fields configured.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            vault_path: None,
            db: None,
            session_cache: None,
            secret: None,
            webauthn: None,
        }
    }

    /// Sets the path to the vault directory where blobs are stored.
    ///
    /// This field is required. Calling `build()` without setting it will panic.
    #[must_use]
    pub fn vault_path<P>(mut self, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.vault_path = Some(path.into());
        self
    }

    /// Sets the `SqlitePool` for all database operations.
    ///
    /// This field is required. Calling `build()` without setting it will panic.
    #[must_use]
    pub fn db(mut self, pool: SqlitePool) -> Self {
        self.db = Some(pool);
        self
    }

    /// Sets the persistent session signing key (`K_server`).
    ///
    /// This field is required. Calling `build()` without setting it will panic.
    /// In production, obtain this value via [`Secret::load_or_create`].
    ///
    /// [`Secret::load_or_create`]: crate::auth::Secret::load_or_create
    #[must_use]
    pub const fn secret(mut self, secret: Secret) -> Self {
        self.secret = Some(secret);
        self
    }

    /// Optionally provides a pre-configured `moka` session cache.
    ///
    /// If not set, `build()` will create a default cache with a 10-minute
    /// Time-To-Idle (TTI) policy and a maximum capacity of 10,000 entries.
    /// This default is suitable for most deployments.
    #[must_use]
    pub fn session_cache(
        mut self,
        session_cache: Cache<TokenHash, UserId>,
    ) -> Self {
        self.session_cache = Some(session_cache);
        self
    }

    /// Sets a pre-configured `WebAuthn` relying party instance.
    ///
    /// If not provided, [`build`](Self::build) initializes a default instance configured
    /// for local development with RP ID `"localhost"`, origin `"http://localhost:23342"`,
    /// and relying party name `"Kosh Vault"`.
    ///
    /// In production, initialize this using the parameters configured in the server's
    /// KDL configuration file ([`kosh_core::config::Passkey`]).
    #[must_use]
    pub fn webauthn(mut self, webauthn: Webauthn) -> Self {
        self.webauthn = Some(Arc::new(webauthn));
        self
    }

    /// Consumes the builder and returns the fully configured `AppState`.
    ///
    /// - If `session_cache` was not set, a default `moka` cache is created with
    ///   a 10-minute TTI and 10,000-entry capacity.
    /// - If `webauthn` was not set, a default Relying Party instance configured for
    ///   `localhost:23342` is created.
    /// - The `passkey` challenge cache is initialized with its default 60-second TTL
    ///   and 1,000-entry capacity via [`Passkey::default`].
    ///
    /// # Panics
    /// Panics if `vault_path` or `db` were not configured before calling `build`.
    #[allow(clippy::expect_used)]
    #[must_use]
    pub fn build(self) -> AppState {
        let vault_path =
            self.vault_path.expect("FATAL: vault_path is required!");

        let webauthn = self.webauthn.unwrap_or_else(|| {
            let origin = Url::parse("http://localhost:23342")
                .expect("valid test origin");

            let builder = WebauthnBuilder::new("localhost", &origin)
                .expect("valid webauthn builder")
                .rp_name("Kosh Vault");

            Arc::new(builder.build().expect("valid test webauthn"))
        });

        AppState {
            storage: storage::Service::new(vault_path.clone()),
            db: self.db.expect("FATAL: database pool is required!"),
            session_cache: self.session_cache.unwrap_or_else(|| {
                Cache::builder()
                    .time_to_idle(Duration::from_mins(10))
                    .max_capacity(10_000)
                    .build()
            }),
            ledger: ledger::Handle::spawn(vault_path),
            secret: self.secret.expect("FATAL: no secret provided??"),
            pow_secret: Secret::random(),

            passkey: Passkey::default(),
            webauthn,
        }
    }
}

impl Default for AppStateBuilder {
    fn default() -> Self {
        Self::new()
    }
}
