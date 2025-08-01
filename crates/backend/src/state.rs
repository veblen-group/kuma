use std::sync::Arc;

use color_eyre::eyre;
use kuma_core::{config::TokenAddressesForChain, database::DatabaseHandle, state::pair::Pair};

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseHandle,
    pub token_configs: Arc<TokenAddressesForChain>,
}

impl AppState {
    pub(crate) fn get_pair_from_str(&self, pair: &str) -> eyre::Result<Pair> {
        unimplemented!()
    }
}
