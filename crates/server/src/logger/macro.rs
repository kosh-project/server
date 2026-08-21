/// Emits an informational log entry through the global logging service.
///
/// This macro is a complete no-op if the logger has not been initialized (i.e., if
/// [`crate::logger::GLOBAL_LOGGER`] is empty). The format string is never evaluated in
/// that case, so there is no runtime cost on the happy path.
///
/// The entry is dispatched via [`tokio::sync::mpsc::Sender::try_send`], which is
/// non-blocking. If the channel is full, the entry is silently dropped rather than
/// blocking the calling request thread.
///
/// # Parameters
///
/// - `$module` — A [`crate::logger::Module`] variant identifying the subsystem emitting
///   the log.
/// - `$($arg)+` — A format string and its arguments, following the same syntax as
///   [`std::format!`].
///
/// # Usage
///
/// ```ignore
/// info!(Module::Api, "request received from user {}", user_id);
/// info!(Module::Storage, "blob committed: {}", hash);
/// ```
#[macro_export]
macro_rules! info {
    ($module:expr, $($arg:tt)+) => {
        if let Some(logger) = $crate::logger::GLOBAL_LOGGER.get() {
            let message = format!($($arg)+);

            let entry = $crate::logger::Entry {
                module: $module,
                level: $crate::logger::Level::Info,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                message,
            };
            let _ = logger.try_send(entry);
        }
	};
}

/// Emits a warning log entry through the global logging service.
///
/// Use this level for abnormal but non-fatal events: authentication failures,
/// request conflicts, resource-not-found responses, or any condition that is
/// unexpected but does not indicate a server malfunction.
///
/// This macro is a complete no-op if the logger has not been initialized.
/// See [`info!`] for parameter documentation and general behaviour.
///
/// # Usage
///
/// ```ignore
/// warn!(Module::Api, "unauthorized access attempt for blob {}", hash);
/// ```
#[macro_export]
macro_rules! warn {
    ($module:expr, $($arg:tt)+) => {
        if let Some(logger) = $crate::logger::GLOBAL_LOGGER.get() {
            let message = format!($($arg)+);

            let entry = $crate::logger::Entry {
                module: $module,
                level: $crate::logger::Level::Warning,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                message,
            };
            let _ = logger.try_send(entry);
        }
	};
}

/// Emits an error log entry through the global logging service.
///
/// Use this level for unexpected failures that affect a single request but do not
/// crash the service: database query failures, storage write errors, or any condition
/// that results in a 5xx response being returned to the client.
///
/// This macro is a complete no-op if the logger has not been initialized.
/// See [`info!`] for parameter documentation and general behaviour.
///
/// # Usage
///
/// ```ignore
/// error!(Module::Database, "query failed: {}", e);
/// ```
#[macro_export]
macro_rules! error {
    ($module:expr, $($arg:tt)+) => {
        if let Some(logger) = $crate::logger::GLOBAL_LOGGER.get() {
            let message = format!($($arg)+);

            let entry = $crate::logger::Entry {
                module: $module,
                level: $crate::logger::Level::Error,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                message,
            };
            let _ = logger.try_send(entry);
        }
	};
}

/// Emits a shutdown sentinel entry and signals the logging service to stop.
///
/// This macro sends an entry with [`crate::logger::Level::Shutdown`] and the
/// module hardcoded to [`crate::logger::Module::Server`]. When the background
/// service receives this entry, it exits its receive loop cleanly.
///
/// This must be called during the server's graceful shutdown sequence, before
/// awaiting [`crate::logger::service::LoggerHandler::shutdown_with_grace`].
///
/// This macro is a no-op if the logger has not been initialized.
///
/// # Usage
///
/// ```ignore
/// // In the graceful shutdown sequence:
/// shutdown!("Waiting to flush remaining entries...");
/// logger_handle.shutdown_with_grace(10).await;
/// ```
#[macro_export]
macro_rules! shutdown {
    ($($arg:tt)+) => {
        if let Some(logger) = $crate::logger::GLOBAL_LOGGER.get() {
            let message = format!($($arg)+);

            let entry = $crate::logger::Entry {
                module: $crate::logger::Module::Server,
                level: $crate::logger::Level::Shutdown,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                message,
            };
            let _ = logger.try_send(entry);
        }
	};
}

/// Emits a fatal log entry through the global logging service.
///
/// Use this level for critical failures that indicate the server may be in an
/// unrecoverable state: the vault directory is missing at startup, a background
/// task has panicked, or a grace period has timed out. A `Fatal` entry does not
/// automatically terminate the process; the caller is responsible for deciding
/// whether to initiate shutdown.
///
/// This macro is a complete no-op if the logger has not been initialized.
/// See [`info!`] for parameter documentation and general behaviour.
///
/// # Usage
///
/// ```ignore
/// fatal!(Module::Server, "grace period timed out, forcefully terminating: {}", e);
/// ```
#[macro_export]
macro_rules! fatal {
    ($module:expr, $($arg:tt)+) => {
        if let Some(logger) = $crate::logger::GLOBAL_LOGGER.get() {
            let message = format!($($arg)+);

            let entry = $crate::logger::Entry {
                module: $module,
                level: $crate::logger::Level::Fatal,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                message,
            };
            let _ = logger.try_send(entry);
        }
    }
}
