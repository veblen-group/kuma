mod models;
mod pair;
mod routes;

pub use models::*;

use std::sync::Arc;

use kuma_core::{config::TokenAddressesForChain, database::DatabaseHandle};

#[derive(Clone)]
pub(crate) struct AppState {
    pub db: DatabaseHandle,
    pub token_configs: Arc<TokenAddressesForChain>,
}
