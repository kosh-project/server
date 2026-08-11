use std::net::Ipv4Addr;

use crate::logger::Entry;
use crate::logger::GLOBAL_LOGGER;
use axum::response::IntoResponse;
use axum::{
    extract::{ConnectInfo, Request},
    middleware::Next,
};

/// Middleware for post-request error telemetry.
///
/// This middleware runs after the route handler has completed and inspects the
/// response for a [`crate::logger::Entry`] that may have been inserted by
/// [`crate::Error::into_response`]. If an entry is present and the global logger
/// is active, it prepends the client IP address, HTTP method, and request path
/// to the message and dispatches the entry to the logging service.
///
/// This function is only attached to the router when [`crate::logger::logging_enabled`]
/// returns `true`. When the logger is inactive the entire middleware layer is absent
/// from the router, so no overhead is incurred.
///
/// # Design: zero-allocation on the happy path
///
/// The HTTP method (`Method`) and URI (`Uri`) are cloned before the handler runs.
/// Both types are cheap to clone (`Method` is a small enum, `Uri` is backed by an
/// `Arc`-managed buffer). The format string that prepends the request context to the
/// log message is only allocated inside the `if let` block, which is only entered
/// when an error entry is actually present in the response extensions.
pub async fn log_middleware(
    ConnectInfo(addr): ConnectInfo<Ipv4Addr>,
    request: Request,
    next: Next,
) -> impl IntoResponse {
    let method = request.method().clone();
    let path = request.uri().clone();

    let mut response = next.run(request).await;

    if let Some(mut entry) = response.extensions_mut().remove::<Entry>()
        && let Some(sender) = GLOBAL_LOGGER.get()
    {
        entry.message = format!(
            "[{}] {} {} FAILED:\n{}",
            addr, method, path, entry.message
        );

        let _ = sender.try_send(entry);
    }

    response
}
