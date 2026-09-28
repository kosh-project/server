use std::{
    net::{IpAddr, SocketAddr},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    api::{Error, Result},
    app,
    error::internal,
};
use axum::{
    Json,
    extract::{ConnectInfo, Query, State},
};
use blake3::{Hasher, keyed_hash};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ChallengeQuery {
    pub action: String,
}

#[derive(Serialize)]
pub struct Response {
    pub challenge: String,
}

pub async fn generate(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<app::State>,
    Query(query): Query<ChallengeQuery>,
) -> Result<Json<Response>> {
    if query.action != "login" && query.action != "register" {
        return Err(Error::BadRequest("Invalid action".into()));
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(internal::Error::from)?
        .as_secs();

    let timestamp_hex = format!("{now :016x}");

    let mut hasher = Hasher::new_keyed(&state.pow_secret.key());

    hasher.update(timestamp_hex.as_bytes());

    match addr.ip() {
        IpAddr::V4(ip) => hasher.update(&ip.octets()),
        IpAddr::V6(ip) => hasher.update(&ip.octets()),
    };
    hasher.update(query.action.as_bytes());

    let mac = hasher.finalize();

    let challenge = format!("{now :016x}{}{}", mac.to_hex(), query.action);

    Ok(Json(Response { challenge }))
}
