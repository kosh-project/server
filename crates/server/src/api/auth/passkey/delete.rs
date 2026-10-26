//! Route handler for revoking and deleting registered passkeys.

use axum::{
    Extension,
    extract::{Path, State},
};
use hyper::StatusCode;

use crate::{
    Result,
    api::Error::{BadRequest, NotFound},
    app,
    model::passkey,
};

/// Revokes and permanently deletes a passkey credential belonging to the authenticated caller.
///
/// Route: `DELETE /api/v1/auth/passkey/{credential_id}` (protected by `mac_guard` and `auth_guard`).
///
/// 1. Decodes the URL path parameter `credential_id` from hexadecimal format into raw bytes.
/// 2. Performs a scoped deletion query in the `webauthn_credentials` table matching both
///    `credential_id` and the caller's `user_id`. This prevents cross-user credential tampering.
/// 3. Returns HTTP `204 No Content` on successful deletion.
///
/// # Errors
///
/// - Returns [`BadRequest`] if `credential_id` is not a valid hex-encoded string.
/// - Returns [`NotFound`] if no passkey matches the given ID or if it belongs to another user.
/// - Returns [`crate::model::Error::Database`] if the `SQLite` delete query fails.
pub async fn delete(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Path(credential_id): Path<String>,
) -> Result<StatusCode> {
    let credential_id = hex::decode(credential_id)
        .map_err(|_| BadRequest("credential_id must be valid hex"))?;

    let deleted =
        passkey::Record::delete(&state.db, &credential_id, user_id).await?;

    if !deleted {
        Err(NotFound("Passkey credential not found"))?;
    }

    Ok(StatusCode::NO_CONTENT)
}
