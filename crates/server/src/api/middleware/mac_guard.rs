use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use blake3::Hasher;

use crate::{
    api::{
        Error::{BadRequest, Unauthorized},
        Result,
    },
    app,
    model::session::{Session, TokenHash},
};

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

    Session::verify_stateless(token, &state.secret)?;

    let token_hash: TokenHash =
        Hasher::new().update(token.as_bytes()).finalize().into();

    request.extensions_mut().insert(token_hash);

    Ok(next.run(request).await)
}
