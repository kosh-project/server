use axum::{
    Json, Router,
    extract::Multipart,
    response::IntoResponse,
    routing::{get, post},
};
use hyper::StatusCode;
use serde_json::{Value, json};
use tokio::{
    io::{self, AsyncWriteExt},
    net::TcpListener,
};

use std::fmt::Display;





#[tokio::main]
async fn main() -> io::Result<()> {
    tokio::fs::create_dir_all("./vault").await?;

    debug("FS", "Initialized vault");

    let app = Router::new()
        .route("/health", get(get_health))
        .route("/storage", get(get_storage))
        .route("/upload", post(handle_upload));

    let listener = TcpListener::bind("0.0.0.0:6969").await?;

    debug("SERVER", "Initiated");
    tokio::select! {
        _ = axum::serve(listener, app) => {

        },
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\nShutting down!");
        }
    }

    Ok(())
}

