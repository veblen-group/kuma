use serde::{Deserialize, Serialize};

use crate::{
    chain::Chain,
    state::{PoolId, pair::Pair},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotPrices {
    pub pair: Pair,
    pub block_height: u64,
    pub min_price: f64,
    pub max_price: f64,
    pub min_pool_id: PoolId,
    pub max_pool_id: PoolId,
    pub chain: Chain,
}
