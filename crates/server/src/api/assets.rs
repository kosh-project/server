use crate::{
    Error::ApiError,
    api::Error::{BadRequest, InvalidHeader, NotFound},
    app::State as AppState,
    error, info,
    logger::Module,
    model::asset::{Asset, AssetTag},
    storage::Payload,
};
use axum::{
    Extension, Json,
    body::Body,
    extract::{Path, State},
    response::IntoResponse,
};
use hyper::{HeaderMap, StatusCode};
use serde::Serialize;
use tokio_util::io::ReaderStream;

use crate::Result;

/// Handles asset deletion requests (DELETE `/api/v1/assets/{hash}`).
///
/// This endpoint removes the user's ownership of the specified asset hash.
/// If the asset is no longer owned by any user, the underlying physical file
/// is deleted from the storage vault.
///
/// # Errors
/// - Returns a `BadRequest` if the hash string cannot be decoded from hex.
/// - Returns an internal error if the database transaction or file deletion fails.
pub async fn delete(
    State(state): State<AppState>,
    Extension(user_id): Extension<i64>,
    Path(hash_str): Path<String>,
) -> Result<impl IntoResponse> {
    let hash_bytes = hex::decode(&hash_str)
        .map_err(|_| BadRequest("Invalid Hash Format".into()))?;

    let wipe_needed =
        Asset::delete(&state.db, user_id, &hash_bytes).await?;

    if wipe_needed {
        state.storage.delete_blob(&hash_str).await?;
    }

    info!(
        Module::Asset,
        "deleted ownership over blob \"{hash_str}\" with success"
    );
    Ok(StatusCode::NO_CONTENT)
}

/// Handles asset download requests (GET `/api/v1/assets/{hash}`).
///
/// Verifies that the requesting user owns the asset with the specified hash,
/// and if authorized, streams the physical file back to the client.
///
/// # Errors
/// - Returns a `BadRequest` if the hash string cannot be decoded from hex.
/// - Returns a `NotFound` if the user does not own the asset or if the physical file is missing.
/// - Returns an internal error if the database query fails.
///
/// # Panics
/// This function panics if the hardcoded "application/octet-stream" content type cannot be parsed.
pub async fn get(
    State(state): State<AppState>,
    Extension(user_id): Extension<i64>,
    Path(hash_str): Path<String>,
) -> Result<impl IntoResponse> {
    let hash_bytes = hex::decode(&hash_str)
        .map_err(|_| BadRequest("Invalid Hash Format".into()))?;

    let owns_file =
        Asset::owned_by(&state.db, user_id, &hash_bytes).await?;

    if !owns_file {
        error!(
            Module::Asset,
            "Attempt to access unauthorized blob '{hash_str}' by user {user_id}"
        );
        return Err(ApiError(NotFound(
            "Asset not found or Unauthorized".into(),
        )));
    }

    let file = state
        .storage
        .get_blob(&hash_str)
        .await
        .map_err(|_| NotFound("File Missing".into()))?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        "application/octet-stream"
            .parse()
            .map_err(|e| ApiError(InvalidHeader(e)))?,
    );

    Ok((StatusCode::OK, headers, body))
}

#[derive(Serialize)]
#[serde(untagged)]
enum FileStatus {
    Success { file_name: String, hash: String },
    Failure { file_name: String, error: String },
}

impl FileStatus {
    const fn success(file_name: String, hash: String) -> Self {
        Self::Success { file_name, hash }
    }

    fn failure(file_name: String, error: &impl ToString) -> Self {
        Self::Failure {
            file_name,
            error: error.to_string(),
        }
    }
}

/// Handles streaming asset uploads (POST `/api/v1/upload/{tag}`).
///
/// This endpoint processes raw binary streams from the client, calculates the BLAKE3 hash
/// incrementally, and commits the file to the Content-Addressable Storage (CAS) vault.
/// The uploaded file's metadata is then recorded in the database.
///
/// # Errors
/// - Returns a `BadRequest` if required headers (`X-File-Name`, `Content-Length`) are missing or malformed.
/// - Returns a `BadRequest` if the payload size exceeds the 10GB limit.
/// - Returns an internal error if the storage transaction or database insertion fails.
pub async fn upload(
    State(state): State<AppState>,
    Path(tag_str): Path<String>,
    headers: HeaderMap,
    Extension(user_id): Extension<i64>,
    body: Body,
) -> crate::Result<impl IntoResponse> {
    let tag = AssetTag::try_from(tag_str.as_str())
        .map_err(|()| BadRequest("Invalid Tag".into()))?;

    let file_name = headers
        .get("X-File-Name")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            BadRequest("Missing X-File-Name header".into())
        })?;

    let expected_size: u64 = headers
        .get("Content-Length")
        .and_then(|x| x.to_str().ok())
        .and_then(|x| x.parse().ok())
        .ok_or_else(|| {
            BadRequest("Missing content length in header".into())
        })?;

    if expected_size > 10_000_000_000 {
        return Err(BadRequest("Payload too Large".into()).into());
    }

    let f_stream = body.into_data_stream();

    let status = match state
        .storage
        .try_save(file_name, Payload::new(expected_size, f_stream))
        .await
    {
        Ok(metadata) => {
            match Asset::create(&state.db, user_id, tag, &metadata)
                .await
            {
                Ok(()) => {
                    info!(
                        Module::Asset,
                        "upload success, user owns {}",
                        metadata.hash.to_string()
                    );
                    FileStatus::success(
                        file_name.into(),
                        metadata.hash.to_string(),
                    )
                }
                Err(e) => {
                    info!(
                        Module::Asset,
                        "user {user_id} failed to register asset ownership to database '{file_name}': {e}"
                    );
                    FileStatus::failure(file_name.into(), &e)
                }
            }
        }

        Err(e) => {
            info!(
                Module::Asset,
                "user {user_id} failed to uplaod '{file_name}'. Failed to write this blob to disk with error {e}"
            );
            FileStatus::failure(file_name.into(), &e)
        }
    };

    Ok(Json(status))
}
