use tokio::{
    io::{self},
    net::TcpListener,
};
use webdav_server::{
    api::route::route_main, app::AppStateBuilder, log,
};

use sqlx::sqlite::SqlitePoolOptions; // You need this import!

#[tokio::main]
async fn main() -> io::Result<()> {
    tokio::fs::create_dir_all("./test/vault").await?;
    log!("FS", "Initialized vault");

    #[allow(clippy::expect_used)]
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite://test/vault/metadata.db")
        .await
        .expect("Failed to connect to SQLite!");

    let app_state = AppStateBuilder::new()
        .db(pool)
        .vault_path(std::path::PathBuf::from("./vault"))
        .build();

    let app = route_main(app_state);

    let listener = TcpListener::bind("0.0.0.0:6969").await?;
    log!("SERVER", "Listening on port 6969");

    tokio::select! {
        _ = axum::serve(listener, app) => {},
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\nKeyboard Interrupt received, Shutting Down!");
        }
    }

    Ok(())
}
