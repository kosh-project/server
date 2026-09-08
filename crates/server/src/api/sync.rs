//! HTTP handlers for the Delta-CRDT sync ledger endpoints.
//!
//! These three handlers expose the append-only ledger to Android clients and
//! allow them to synchronize encrypted CRDT action events across devices.
//!
//! ## Endpoints
//!
//! | Method | Path | Handler |
//! |--------|------|---------|
//! | `POST` | `/api/v1/sync/delta` | [`append_delta`] |
//! | `GET` | `/api/v1/sync/delta` | [`stream_delta`] |
//! | `DELETE` | `/api/v1/sync/prune` | [`prune_ledger`] |
//!
//! All endpoints require a valid Bearer token and are placed behind
//! `auth_guard` in `api/route.rs`.
//!
//! ## Framing
//!
//! The server treats every payload as an opaque byte blob. It never parses
//! the encrypted content. However, `append_delta` prepends a 4-byte
//! little-endian length prefix to each payload before writing it to disk:
//!
//! ```text
//! [4 bytes LE: payload length][N bytes: encrypted payload]
//! ```
//!
//! This framing is the convention shared with the Android client. When the
//! client calls `stream_delta`, it can use the length prefix to read one
//! action at a time from the raw byte stream.

use axum::body::Body;
use axum::extract::Query;
use axum::response::{IntoResponse, Response};
use axum::{Extension, extract::State};
use axum::{Json, body};
use bytes::Bytes;
use hyper::{HeaderMap, StatusCode, header};
use serde::{Deserialize, Serialize};
use tokio_util::io::ReaderStream;

use crate::api::{Error, Result};
use crate::{app, storage::ledger::AppendReciept};

/// The JSON response body returned by [`append_delta`].
///
/// These two fields form the **high-water mark cursor** that the client stores
/// after each successful upload. On the next sync session the client passes
/// them back as query parameters to [`stream_delta`] to resume streaming from
/// exactly where it left off, with no gap and no duplication.
#[derive(Serialize)]
pub struct AppendResponse {
    /// Name of the segment file the payload was written to, e.g. `"delta_0000003"`.
    pub file_name: String,
    /// Byte offset immediately after the last written byte in that segment.
    ///
    /// This is an **end offset**, not a start offset. Passing this value as
    /// the `offset` query parameter in a subsequent `GET /sync/delta` request
    /// will begin streaming from the first byte after the data just uploaded.
    pub offset: u64,
}

impl From<AppendReciept> for AppendResponse {
    fn from(reciept: AppendReciept) -> Self {
        Self {
            file_name: reciept.file_name,
            offset: reciept.offset,
        }
    }
}

/// `POST /api/v1/sync/delta`
///
/// Accepts a raw encrypted payload from the client and appends it to that
/// user's active ledger segment.
///
/// ## Framing
///
/// Before passing the bytes to the ledger actor, this handler prepends a
/// 4-byte little-endian length value equal to the body size. The ledger
/// domain layer itself is framing-agnostic and stores whatever bytes it
/// receives verbatim.
///
/// ## Response
///
/// Returns `200 OK` with an [`AppendResponse`] JSON body containing the
/// segment name and high-water mark offset.
///
/// ## Errors
///
/// Propagates any [`crate::storage::ledger::Error`] returned by the actor,
/// including `500 Internal Server Error` if the actor task has terminated
/// unexpectedly.
pub async fn append_delta(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    body: Bytes,
) -> Result<Json<AppendResponse>> {
    let len = body.len() as u32;

    let mut payload = Vec::with_capacity(2 + body.len());
    payload.extend_from_slice(&len.to_le_bytes());
    payload.extend_from_slice(&body);

    let receipt = state.ledger.append(user_id, Bytes::from(payload)).await?;

    Ok(Json(receipt.into()))
}

/// Query parameters accepted by [`stream_delta`].
#[derive(Deserialize)]
pub struct SyncRequest {
    /// The segment filename to read from, e.g. `"delta_0000003"`.
    ///
    /// Must start with `"delta_"` and must not contain path separators.
    pub file: String,
    /// The byte offset to start streaming from within the segment.
    ///
    /// The server silently clamps this to a minimum of `500` (the KOSH
    /// header size) so that clients passing `0` are equivalent to clients
    /// passing `500`. See [`Handle::read_segment`] for the full offset
    /// validation logic.
    ///
    /// [`Handle::read_segment`]: crate::storage::ledger::handle::Handle::read_segment
    pub offset: u64,
}

use crate::api::Error::InvalidHeader;

/// `GET /api/v1/sync/delta`
///
/// Streams the contents of a specific ledger segment starting from a given
/// byte offset. The response body is a raw `application/octet-stream`.
///
/// The client reads the returned bytes and uses the 4-byte length prefixes
/// written by [`append_delta`] to deserialize individual encrypted actions.
///
/// ## Path traversal protection
///
/// The `file` query parameter is validated at this handler level before being
/// passed to [`Handle::read_segment`], which applies the same check again as
/// a second line of defense. Requests with a filename that contains `/`, `\`,
/// or does not start with `"delta_"` are rejected with `400 Bad Request`
/// immediately, without touching the filesystem.
///
/// ## Errors
///
/// | Condition | Status |
/// |-----------|--------|
/// | Invalid `file` name | `400 Bad Request` |
/// | Segment does not exist | `404 Not Found` |
/// | `offset` out of bounds | `400 Bad Request` |
/// | Actor is dead | `500 Internal Server Error` |
///
/// [`Handle::read_segment`]: crate::storage::ledger::handle::Handle::read_segment
pub async fn stream_delta(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Query(query): Query<SyncRequest>,
) -> Result<impl IntoResponse> {
    if query.file.contains('\\')
        || query.file.contains('/')
        || !query.file.starts_with("delta_")
    {
        return Err(Error::BadRequest("Invalid file name".into()));
    }

    let file = state
        .ledger
        .read_segment(state.vault_path(), user_id, &query.file, query.offset)
        .await?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        "application/octet-stream".parse().map_err(InvalidHeader)?,
    );

    Ok((StatusCode::OK, headers, body))
}

/// Query parameters accepted by [`prune_ledger`].
#[derive(Deserialize)]
pub struct PruneRequest {
    /// All segments with a numeric ID strictly less than this value will be deleted.
    pub before: u32,
}

/// `DELETE /api/v1/sync/prune`
///
/// Instructs the ledger actor to delete all delta segments for the
/// authenticated user whose segment ID is strictly less than `before`.
///
/// This is called by the Android client after it has confirmed that all
/// devices it manages have consumed and applied the events in those old
/// segments. Pruning keeps the per-user ledger directory from growing
/// indefinitely.
///
/// ## Safety
///
/// The actor enforces the invariant that the active segment is never deleted,
/// even if the `before` value happens to equal the active segment's ID. Passing
/// a `before` value **greater than** the active segment ID is rejected with
/// `400 Bad Request`.
///
/// ## Response
///
/// Returns `200 OK` with an empty body on success.
pub async fn prune_ledger(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Query(query): Query<PruneRequest>,
) -> Result<impl IntoResponse> {
    state.ledger.prune(user_id, query.before).await?;

    Ok(StatusCode::OK)
}


#[cfg(test)]
mod tests {
    use anyhow::Result;
    use axum::{
        Extension,
        extract::{Query, State},
        response::IntoResponse,
    };
    use bytes::Bytes;
    use http_body_util::BodyExt;
    use hyper::StatusCode;
    use sqlx::SqlitePool;
    use std::{any, assert_matches};
    use tmpdir::TmpDir;

    use crate::{
        api::{
            Error,
            sync::{
                PruneRequest, SyncRequest, append_delta, prune_ledger,
                stream_delta,
            },
        },
        app::{self, AppStateBuilder},
    };

    async fn setup_env() -> Result<(TmpDir, app::State)> {
        let tmp = TmpDir::new("api_sync").await?;

        let pool = SqlitePool::connect("sqlite::memory:").await?;

        let state = AppStateBuilder::new()
            .vault_path(tmp.to_path_buf())
            .db(pool)
            .build();

        Ok((tmp, state))
    }

    #[tokio::test]
    async fn api_delta_append_success() -> anyhow::Result<()> {
        let (_tmp, state) = setup_env().await?;

        let user_id = 99;
        let payload = Bytes::from("PAYLOAD PAYLOAD");

        let result =
            append_delta(State(state), Extension(user_id), payload.clone())
                .await?;

        let reciept = result.0;

        assert_eq!(reciept.file_name, "delta_0000001");
        assert_eq!(reciept.offset, 500 + 4 + payload.len() as u64);

        Ok(())
    }

    #[tokio::test]
    async fn api_stream_success() -> anyhow::Result<()> {
        let (_tmp, state) = setup_env().await?;
        let user_id = 99;

        let _ = append_delta(
            State(state.clone()),
            Extension(user_id),
            Bytes::from("PAYLOAD"),
        )
        .await?;

        let query = Query(SyncRequest {
            file: "delta_0000001".into(),
            offset: 0,
        });

        let response =
            stream_delta(State(state.clone()), Extension(user_id), query)
                .await?
                .into_response();

        assert_eq!(response.status(), 200);

        let body_bytes = response.into_body().collect().await?.to_bytes();
        let len = u32::from_le_bytes(body_bytes[0..4].try_into()?);

        assert_eq!(len, 7);
        assert_eq!(&body_bytes[4..], b"PAYLOAD");

        Ok(())
    }

    #[tokio::test]
    async fn api_stream_path_traversal() -> anyhow::Result<()> {
        let (_tmp, state) = setup_env().await?;
        let user_id = 99;

        let query = Query(SyncRequest {
            file: "../../../etc/passwd".to_string(),
            offset: 0,
        });

        let res = stream_delta(State(state), Extension(user_id), query).await;

        assert!(matches!(res, Err(Error::BadRequest(_))));

        Ok(())
    }

    #[tokio::test]
    async fn api_delta_not_found() -> anyhow::Result<()> {
        let (_tmp, state) = setup_env().await?;
        let user_id = 99;

        let query = Query(SyncRequest {
            file: "delta_00000099".to_string(),
            offset: 500,
        });

        let res = stream_delta(State(state), Extension(user_id), query).await;

        let response = res.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        Ok(())
    }

    #[tokio::test]
    async fn api_delta_prune_success() -> anyhow::Result<()> {
        let (_tmp, state) = setup_env().await?;
        let user_id = 99;

        let query = Query(PruneRequest { before: 1 });

        let res = prune_ledger(State(state), Extension(user_id), query).await?;

        let response = res.into_response();

        assert_eq!(response.status(), StatusCode::OK);
        Ok(())
    }
}
