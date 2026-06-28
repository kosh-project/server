use crate::{
    api::Error::{BadRequest, Unauthorized},
    app::State as AppState,
    model::session::Session,
};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use blake3::Hasher;

use crate::Result;

pub async fn auth_guard(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let header = request
        .headers()
        .get("Authorization")
        .ok_or(Unauthorized("Missing Header".into()))?;

    let token = header
        .to_str()
        .map_err(|_| {
            BadRequest("Auth failed to serialize".into())
        })?
        .strip_prefix("Bearer ")
        .ok_or(Unauthorized(
            "Tokens must start with 'Bearer'".into(),
        ))?;

    let token_hash = Hasher::new()
        .update(token.as_bytes())
        .finalize()
        .as_bytes()
        .to_vec();

    let session =
        Session::verify(&state.db, token_hash.as_ref())
            .await?
            .ok_or(Unauthorized(
                "Invalid or expired session".into(),
            ))?;

    request.extensions_mut().insert(session.user_id);

    Ok(next.run(request).await)
}
