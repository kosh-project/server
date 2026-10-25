use axum::{Extension, Json, extract::State};

use crate::{app, error::JsonResult, model::passkey};

pub async fn list(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
) -> JsonResult<Vec<passkey::Summary>> {
    let list = passkey::Record::list_for_user(&state.db, user_id).await?;

    Ok(Json(list))
}
