use kuma_core::{config::TokenAddressesForChain, database::DatabaseHandle};

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseHandle,
    pub token_configs: TokenAddressesForChain,
}

impl AppState {
    pub(crate) fn get_pair_from_str(&self, pair: &str) -> eyre::Result<Pair> {
        unimplemented!()
    }
}
