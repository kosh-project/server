use crate::api::Error::*;
use axum::{
    Json,
    extract::{Multipart, State},
    response::IntoResponse,
};
use serde_json::json;

use crate::{app::State as AppState, log};

pub(crate) async fn handle_upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> core::result::Result<impl IntoResponse, crate::Error> {
    log!("HANDLER", "post_upload");

    // let join_set = JoinSet::new();

    while let Some(field) = multipart.next_field().await.map_err(|_| StreamReadError)? {
        let file_name = field.file_name().unwrap_or("unnamed_file").to_string();
        log!("MULTI_PART", format!("field : {file_name}"));

        state.storage.try_save(&file_name, field).await?;
    }

    Ok(Json(json! ({
        "diddy_do_it?" : true
    })))
}
