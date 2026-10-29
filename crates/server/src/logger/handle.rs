use std::time::Duration;

use kosh_core::logger::Module;
use tokio::{task::JoinHandle, time::timeout};

use crate::fatal;

/// A handle to the background logging task.
///
/// Returned by [`Service::start`] alongside the channel sender. The caller should
/// retain this handle and use it during the server's graceful shutdown sequence to
/// ensure that all buffered log entries are flushed to disk before the process exits.
pub struct Handle(pub JoinHandle<()>);

impl Handle {
    /// Waits for the logging task to finish, with a timeout.
    ///
    /// Before calling this method, the caller must send a [`kosh_core::logger::Level::Shutdown`]
    /// entry through the channel (typically via the [`crate::shutdown!`] macro) to signal the
    /// service to exit its receive loop. This method then waits up to `secs` seconds for
    /// the task to join.
    ///
    /// If the task does not finish within the grace period, a [`kosh_core::logger::Level::Fatal`]
    /// log entry is emitted (which will itself be silently dropped if the sender is gone) and the
    /// method returns, allowing the OS to clean up the task.
    pub async fn shutdown_with_grace(self, secs: u64) {
        if let Err(e) = timeout(Duration::from_secs(secs), self.0).await {
            fatal!(
                Module::Logger,
                "Grace period of {secs} secs, timed out, forcefully terminating engine.\n{e}"
            );
        }
    }
}
