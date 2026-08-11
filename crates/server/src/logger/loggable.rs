use crate::logger::{Level, Module};

/// Allows an error type to declare its own log severity and module routing.
///
/// Implementing this trait on a domain error type decentralizes the decision of how
/// to categorize an error for telemetry purposes. The error itself knows best whether
/// it represents a minor client mistake (a `Warning`) or a critical infrastructure
/// failure (a `Fatal`). This avoids the need for a centralized `match` statement in
/// the middleware every time a new error variant is added.
///
/// ## Implementation contract
///
/// Implementations must be pure and infallible. They inspect `&self` and return a
/// static classification. They must not allocate or perform any I/O.
///
/// ## Usage
///
/// This trait is consumed by [`crate::Error::into_response`], which calls `log_level`
/// and `log_module` on the top-level error *before* consuming it (since `into_response`
/// takes `self` by value). The resulting values are packed into a [`crate::logger::Entry`]
/// and placed into the response extension backpack for the logging middleware to pick up.
///
/// ```rust
/// use webdav_server::logger::{Level, Module, Loggable};
///
/// struct MyError;
///
/// impl Loggable for MyError {
///     fn log_level(&self) -> Level {
///         Level::Error
///     }
///
///     fn log_module(&self) -> Module {
///         Module::Api
///     }
/// }
/// ```
pub trait Loggable {
    /// Returns the severity level that should be assigned to this error in the log.
    fn log_level(&self) -> Level;

    /// Returns the server module responsible for this error.
    ///
    /// This value is used by the admin CLI to filter and group entries. Most domain
    /// errors return a fixed module (e.g., `Module::Storage` for all `storage::Error`
    /// variants), while the top-level `crate::Error` delegates to the inner error.
    fn log_module(&self) -> Module;
}
