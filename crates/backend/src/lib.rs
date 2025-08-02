pub mod models;
pub mod pair;
mod routes;

use axum::Router;
use color_eyre::eyre::{self, eyre};
use routes::spot_prices;
use tower_http::cors::CorsLayer;
use tracing::info;

use std::{net::SocketAddr, sync::Arc};

use kuma_core::{
    config::{Config, TokenAddressesForChain},
    database::{DatabaseBuilder, DatabaseHandle},
};

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseHandle,
    pub token_configs: Arc<TokenAddressesForChain>,
}

pub async fn spawn(config: Config) -> eyre::Result<()> {
    let db_handle = DatabaseBuilder {
        config: config.database.clone(),
    }
    .build()
    .await?;

    let (token_configs, _) = config
        .build_addrs_and_inventory()
        .map_err(|e| eyre!("failed to parse chain assets: {}", e))?;
    let state = AppState {
        db: db_handle,
        token_configs: Arc::new(token_configs),
    };
    let cors = CorsLayer::permissive();

    let app = Router::new()
        .nest("/spot_prices", spot_prices::routes())
        .nest("/signals", routes::signals::routes())
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], config.server.port));
    info!("🚀 Kuma API server running at http://{addr}");

    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    axum::serve(listener, app)
        .await
        .map_err(|e| eyre!("axum server failed: {e:}"))
}
