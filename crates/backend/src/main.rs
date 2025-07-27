mod config;
mod database;
mod models;
mod routes;
mod state;

use axum::Router;
use color_eyre::eyre::Result;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::{
    config::Config,
    database::DatabaseBuilder,
    routes::{signals, spot_prices},
    state::AppState,
};

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    let config = Config::load()?;
    let db_handle = DatabaseBuilder::new(config.database).build().await?;

    let state = AppState { db: db_handle };
    let cors = CorsLayer::permissive();

    let app = Router::new()
        .nest("/spot_prices", spot_prices::routes())
        .nest("/signals", signals::routes())
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], config.server.port));
    info!("🚀 Kuma API server running at http://{addr}");

    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    axum::serve(listener, app).await?;

    Ok(())
}
