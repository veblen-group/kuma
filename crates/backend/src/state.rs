use std::sync::Arc;

use kuma_core::{config::TokenAddressesForChain, database::DatabaseHandle};

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseHandle,
    pub token_configs: Arc<TokenAddressesForChain>,
}
