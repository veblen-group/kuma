use std::fmt::Display;

use color_eyre::eyre::{self, eyre};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    chain::Chain,
    state::{
        PoolId,
        pair::{Pair, PairState},
    },
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
        let min = precompute.sorted_prices_a_b[0].clone();
        let max = precompute.sorted_prices_a_b[precompute.sorted_prices_a_b.len() - 1].clone();
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

    pub fn try_from_sorted_prices(
        sorted_spot_prices: &[(PoolId, f64)],
        block_height: u64,
        chain: Chain,
        pair: Pair,
    ) -> eyre::Result<Self> {
        if sorted_spot_prices.is_empty() {
            return Err(eyre!("no spot prices provided"));
        }
        let min = sorted_spot_prices[0].clone();
        let max = sorted_spot_prices[sorted_spot_prices.len() - 1].clone();
        Ok(SpotPrices {
            pair,
            block_height,
            min_pool_id: min.0,
            min_price: min.1,
            max_pool_id: max.0,
            max_price: max.1,
            chain,
        })
    }

    pub fn try_from_pair_state(state: &PairState, pair: Pair, chain: Chain) -> eyre::Result<Self> {
        let sorted_spot_prices = try_make_sorted_spot_prices(state, &pair)?;

        SpotPrices::try_from_sorted_prices(&sorted_spot_prices, state.block_height, chain, pair)
    }

    pub fn try_pessimistic_usdc_price(&self) -> eyre::Result<f64> {
        if "USDC" == self.pair.token_a().symbol {
            Ok(self.min_price)
        } else if "USDC" == self.pair.token_b().symbol {
            Ok(self.max_price)
        } else {
            Err(eyre!("not a USDC pair"))
        }
    }
}

impl Display for SpotPrices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Pair: {}, Block Height: {}, Min Price: {}, Max Price: {}",
            self.pair, self.block_height, self.min_price, self.max_price
        )
    }
}

// NOTE: these are analogous to midprice
pub fn try_make_sorted_spot_prices(
    state: &PairState,
    pair: &Pair,
) -> eyre::Result<Vec<(PoolId, f64)>> {
    let mut spot_prices: Vec<(PoolId, f64)> = state
        .states
        .iter()
        .filter_map(|(id, pool)| {
            let spot_price = pool.spot_price(pair.token_a(), pair.token_b());
            match spot_price {
                Ok(price) => Some((id.clone(), price)),
                Err(err) => {
                    warn!(
                        error = %err,
                        pair = %pair,
                        "failed to get spot price, skipping pool"
                    );
                    None
                }
            }
        })
        .collect();

    if spot_prices.is_empty() {
        return Err(eyre::eyre!("no spot prices found"));
    }

    spot_prices
        .sort_by(|(_, spot_price), (_, other_spot_price)| spot_price.total_cmp(other_spot_price));
    Ok(spot_prices)
}
