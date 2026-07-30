use std::io::ErrorKind;

use axum::{
    Extension, Json, body::Body, extract::{Path, State}, response::IntoResponse,
};
use hyper::{HeaderMap, StatusCode};
use serde::Serialize;
use tokio_util::io::ReaderStream;
use crate::{
    Error::ApiError, api::Error::{BadRequest, NotFound}, app::State as AppState, log, model::asset::{Asset, AssetTag}, storage::Payload,
};

use crate::Result;

/// DELETE /api/v1/assets/{hash}
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

    Ok(StatusCode::NO_CONTENT)
}


/// GET /api/v1/assets/hash
pub async fn get(
    State(state): State<AppState>,
    Extension(user_id) : Extension<i64>,
    Path(hash_str): Path<String>,
) -> Result<impl IntoResponse> {
    let hash_bytes = hex::decode(&hash_str)
    .map_err(|_| BadRequest("Invalid Hash Format".into()))?;

    let owns_file = Asset::owned_by(&state.db, user_id, &hash_bytes).await?;

    if !owns_file {
        return Err(ApiError(NotFound("Asset not found or Unauthorized".into())))
    }

    let file = state.storage.get_blob(&hash_str).await.map_err(|_| NotFound("File Missing".into()))?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "application/octet-stream".parse().unwrap());

    Ok((StatusCode::OK, headers, body))
}

#[derive(Serialize)]
#[serde(untagged)]
enum FileStatus {
    Success { file_name: String, hash: String },
    Failure { file_name: String, error: String },
}

impl FileStatus {
    fn success(file_name: String, hash: String) -> Self {
        Self::Success { file_name, hash }
    }

    fn failure(file_name: String, error: impl ToString) -> Self {
        Self::Failure {
            file_name,
            error: error.to_string(),
        }
    }
}

pub async fn upload(
    State(state): State<AppState>,
    Path(tag_str): Path<String>,
    headers: HeaderMap,
    Extension(user_id): Extension<i64>,
    body: Body,
) -> crate::Result<impl IntoResponse> {
    log!("HANDLER", "post_upload");

    let tag = AssetTag::try_from(tag_str.as_str())
        .map_err(|_| BadRequest("Invalid Tag".into()))?;

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
                Ok(_) => FileStatus::success(
                    file_name.into(),
                    metadata.hash.to_string(),
                ),
                Err(e) => FileStatus::failure(file_name.into(), e),
            }
        }
        Err(e) => FileStatus::failure(file_name.into(), e),
    };

    Ok(Json(status))
}
