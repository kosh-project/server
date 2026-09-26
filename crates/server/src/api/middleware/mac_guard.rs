use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use blake3::{Hash, Hasher, keyed_hash};

use crate::{
    api::{
        Error::{BadRequest, Unauthorized},
        Result,
    },
    app,
    model::session::TokenHash,
};

// Expected token length : 101 bytes
const TOKEN_LEN: usize = 36 + 1 + 64;

pub async fn mac_guard(
    State(state): State<app::State>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let header = request
        .headers()
        .get("Authorization")
        .ok_or_else(|| Unauthorized("Missing Header".into()))?;

    let token = header
        .to_str()
        .map_err(|_| BadRequest("Auth failed to serialize".to_owned()))?
        .strip_prefix("Bearer ")
        .ok_or_else(|| {
            Unauthorized("Tokens must start with 'Bearer'".into())
        })?;

    if token.len() != TOKEN_LEN {
        return Err(Unauthorized("Invalid token format".into()));
    }

    let (session_id, mac_hex) = token
        .split_once('.')
        .ok_or_else(|| Unauthorized("Invalid token MAC".into()))?;

    let recieved_mac = Hash::from_hex(mac_hex)
        .map_err(|_| Unauthorized("Invalid token MAC".into()))?;

    let expected_mac = keyed_hash(&state.secret.key(), session_id.as_bytes());

    if expected_mac != recieved_mac {
        return Err(Unauthorized("Forged or invalid token".into()));
    }

    let token_hash: TokenHash = Hasher::new()
        .update(token.as_bytes())
        .finalize()
        .as_bytes()
        .into();

    request.extensions_mut().insert(token_hash);

    Ok(next.run(request).await)
}
