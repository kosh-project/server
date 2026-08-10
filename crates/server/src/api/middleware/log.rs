use std::net::Ipv4Addr;

use crate::logger::Entry;
use crate::logger::GLOBAL_LOGGER;
use axum::response::IntoResponse;
use axum::{
    extract::{ConnectInfo, Request},
    middleware::Next,
};

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
