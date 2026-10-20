use axum::{
    Extension,
    extract::{Query, State},
    response::IntoResponse,
};
use serde::Deserialize;

use crate::{Result, app};

#[derive(Debug, Deserialize)]
pub struct Params {
    pub namespace: String,
    pub before: u32,
}

pub async fn prune_ledger(
    State(state): State<app::State>,
    Extension(user_id): Extension<u64>,
    Query(params): Query<Params>,
) -> Result<impl IntoResponse> {
}
