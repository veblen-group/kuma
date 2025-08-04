pub mod models;
pub mod pair;
mod routes;

use axum::Router;
use color_eyre::eyre::{self, eyre};
use routes::spot_prices;
use tower_http::cors::CorsLayer;
use tracing::info;

use std::sync::Arc;

use kuma_core::{
    config::Config,
    database::{self, Handle},
};

#[derive(Clone)]
pub struct AppState {
    pub db: Handle,
}

pub async fn spawn(config: Config) -> eyre::Result<()> {
    let (token_configs, _) = config
        .build_addrs_and_inventory()
        .map_err(|e| eyre!("failed to parse chain assets: {}", e))?;

    let db_handle =
        database::Handle::from_config(config.database.clone(), Arc::new(token_configs.clone()))?;
    let state = AppState { db: db_handle };
    let cors = CorsLayer::permissive();

    let app = Router::new()
        .nest("/spot_prices", spot_prices::routes())
        .nest("/signals", routes::signals::routes())
        .layer(cors)
        .with_state(state);

    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    info!("🚀 Kuma API server running at http://{bind_addr}");

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    axum::serve(listener, app)
        .await
        .map_err(|e| eyre!("axum server failed: {e:}"))
}
