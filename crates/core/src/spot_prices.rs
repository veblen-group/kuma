use num_bigint::BigUint;
use serde::{Deserialize, Serialize};

use crate::{
    chain::Chain,
    state::{PoolId, pair::Pair},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotPrices {
    pub pair: Pair,
    pub block_height: u64,
    pub min_price: BigUint,
    pub max_price: BigUint,
    pub pool_id: PoolId,
    pub chain: Chain,
}
