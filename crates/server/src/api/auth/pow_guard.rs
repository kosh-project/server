use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Request, State},
    middleware::Next,
    response::Response,
};

use crate::{
    api::{
        Error::{BadRequest, Internal, Unauthorized},
        Result,
        auth::hashcash::HashCash,
    },
    app,
    error::internal::Error::MissingConnectInfo,
};

pub async fn pow_guard(
    State(state): State<app::State>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let header = request
        .headers()
        .get("X-Hashcash")
        .ok_or_else(|| Unauthorized("Missing X-Hashcash header".into()))?
        .to_str()
        .map_err(|_| BadRequest("Invalid Hashcash header format".into()))?;

    if header.len() > 168 {
        return Err(BadRequest("X-Hashcash header too large".into()));
    }

    let ConnectInfo(addr) = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .ok_or(Internal(MissingConnectInfo))?;

    let hashcash =
        HashCash::verify_stateless(header, addr.ip(), &state.pow_secret)?;

    request.extensions_mut().insert(hashcash);

    Ok(next.run(request).await)
}
