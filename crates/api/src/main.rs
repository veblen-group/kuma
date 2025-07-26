mod database;
mod models;
mod routes;
mod state;

use axum::Router;
use color_eyre::eyre::Result;
use std::net::SocketAddr;
use tracing::info;

use crate::{
    database::{DatabaseBuilder, DatabaseConfig},
    routes::{signals, spot_prices},
    state::AppState,
};

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    let db_config = DatabaseConfig::default();
    let (db_worker, db_handle) = DatabaseBuilder::new()
        .with_config(db_config)
        .build()
        .await?;

    let db_task = tokio::spawn(async move {
        if let Err(e) = db_worker.run().await {
            tracing::error!("Database worker failed: {}", e);
        }
    });

    let state = AppState { db: db_handle.clone() };
    
    // Keep the original handle alive to prevent shutdown
    let _db_handle_keeper = db_handle;

    let app = Router::new()
        .nest("/spot_prices", spot_prices::routes())
        .nest("/signals", signals::routes())
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("🚀 Kuma API server running at http://{addr}");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    
    tokio::select! {
        result = axum::serve(listener, app) => {
            result?;
        }
        _ = db_task => {
            tracing::warn!("Database worker ended unexpectedly");
        }
    }

    Ok(())
}
