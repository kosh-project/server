//! Route handler for enumerating registered passkey credentials.

use axum::{Extension, Json, extract::State};

use crate::{app, error::JsonResult, model::passkey};

/// Lists all passkeys registered to the authenticated caller.
///
/// Route: `GET /api/v1/auth/passkey` (protected by `mac_guard` and `auth_guard`).
///
/// Retrieves all passkey records belonging to `user_id` from the `webauthn_credentials`
/// table, sorted newest to oldest by registration timestamp (`created_at DESC`).
///
/// Returns a JSON array of [`passkey::Summary`] objects containing hex-encoded credential IDs,
/// client-encrypted metadata (such as device nicknames), creation timestamps, and
/// last-used timestamps. Raw cryptographic public keys and internal counters are withheld
/// to preserve privacy and minimize payload size.
///
/// # Errors
///
/// - Returns [`crate::model::Error::Database`] if querying `SQLite` fails.
pub async fn list(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
) -> JsonResult<Vec<passkey::Summary>> {
    let list = passkey::Record::list_for_user(&state.db, user_id).await?;

    Ok(Json(list))
}
