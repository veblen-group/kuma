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
    /// Create SpotPrices from precomputed spot price data.
    ///
    /// Extracts the minimum and maximum spot prices from the precomputed sorted prices array
    /// and creates a SpotPrices struct for the given pair and chain.
    ///
    /// # Arguments
    /// * `precompute` - Precomputed data containing sorted spot prices (min to max)
    /// * `chain` - The blockchain network for which these prices apply
    /// * `pair` - The token pair for which spot prices are calculated
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

    /// Create SpotPrices from a pre-sorted list of spot prices.
    ///
    /// Takes a sorted array of (PoolId, price) tuples where the price represents
    /// the amount of token_b per unit of token_a, and extracts the minimum and maximum values.
    ///
    /// # Arguments
    /// * `sorted_spot_prices` - A sorted slice of (PoolId, price) tuples
    /// * `block_height` - The block height at which these prices are valid
    /// * `chain` - The blockchain network for which these prices apply
    /// * `pair` - The token pair for which spot prices are calculated
    ///
    /// # Errors
    /// Returns an error if the sorted_spot_prices slice is empty.
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

    /// Create SpotPrices by extracting and sorting spot prices from a pair's state.
    ///
    /// Retrieves the current spot price for the token pair from all available pools,
    /// sorts them, and creates a SpotPrices struct containing the minimum and maximum prices.
    /// Skips any pools that fail to calculate a valid spot price.
    ///
    /// # Arguments
    /// * `state` - The current state of the pair containing all pool states
    /// * `pair` - The token pair for which spot prices are calculated
    /// * `chain` - The blockchain network for which these prices apply
    ///
    /// # Errors
    /// Returns an error if no valid spot prices can be extracted from the pair state.
    pub fn try_from_pair_state(state: &PairState, pair: Pair, chain: Chain) -> eyre::Result<Self> {
        let sorted_spot_prices = try_make_sorted_spot_prices(state, &pair)?;

        SpotPrices::try_from_sorted_prices(&sorted_spot_prices, state.block_height, chain, pair)
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

/// Extract and sort spot prices for a token pair from all pools in a pair state.
///
/// Iterates through all pools in the pair state and calculates the spot price for each.
/// Pools that fail to calculate a valid spot price are silently skipped with a warning logged.
///
/// The spot prices are analogous to the mid-price across all available pools.
///
/// ## Price direction
///
/// Prices are computed via `ProtocolSim::spot_price(base, quote)`, which returns **quote per base**
/// (i.e. how many `quote` tokens you receive per unit of `base`).
///
/// The base/quote assignment is determined by `Pair::token_a_b_adjusted_for_usdc()`:
/// USDC is always placed as the quote so prices are expressed in USDC terms
/// (e.g. ~3000 USDC/WETH for a USDC-WETH pair). For non-USDC pairs the strategy
/// order is preserved: token_a is base, token_b is quote.
///
/// # Arguments
/// * `state` - The current state of the pair containing all pool states
/// * `pair` - The token pair for which spot prices are calculated
///
/// # Returns
/// A vector of `(PoolId, price)` tuples sorted by price ascending, where price is **quote per base**
///
/// # Errors
/// Returns an error if no valid spot prices can be extracted from any pool.
pub fn try_make_sorted_spot_prices(
    state: &PairState,
    pair: &Pair,
) -> eyre::Result<Vec<(PoolId, f64)>> {
    let mut spot_prices: Vec<(PoolId, f64)> = state
        .states
        .iter()
        .filter_map(|(id, pool)| {
            // base = non-USDC (or token_a); quote = USDC (or token_b).
            // spot_price(base, quote) returns quote-per-base.
            let (token_a, token_b) = pair.token_a_b_adjusted_for_usdc();
            let spot_price = pool.spot_price(token_a, token_b);
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
