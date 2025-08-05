use serde::{Deserialize, Serialize};

use crate::{
    chain::Chain,
    state::{PoolId, pair::Pair},
    strategy::Precomputes,
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

impl SpotPrices {
    pub fn from_precompute(precompute: &Precomputes, chain: Chain, pair: Pair) -> Self {
        let min = precompute.sorted_spot_prices[0].clone();
        let max = precompute.sorted_spot_prices[precompute.sorted_spot_prices.len() - 1].clone();
        SpotPrices {
            pair,
            block_height: precompute.block_height,
            min_pool_id: min.0,
            min_price: min.1,
            max_pool_id: max.0,
            max_price: max.1,
            chain,
        }
    }
}
