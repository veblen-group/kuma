//! Cross-chain single-hop arbitrage strategy implementation.
//!
//! Detects price differences between DEX pools on slow and fast chains for a token pair.
//! Uses precomputed swap simulations on the slow chain and real-time simulation on the fast
//! chain to identify profitable arbitrage opportunities via binary search optimization.

use std::{collections::HashMap, sync::Arc};

use color_eyre::eyre::{self, Context, eyre};
use num_bigint::BigUint;
use tracing::{error, instrument, trace};
use tycho_common::models::token::Token;
use tycho_simulation::{
    protocol::models::ProtocolComponent, tycho_core::simulation::protocol_sim::ProtocolSim,
};

use crate::{
    chain::Chain,
    signals::{self, Direction, bps_discount},
    spot_prices::{SpotPrices, try_make_sorted_spot_prices},
    state::{
        self, PoolId,
        block::BlockState,
        pair::{Pair, PairState},
    },
};

mod builder;
pub mod simulation;
pub use builder::Builder;
pub use simulation::Swap;

/// Precomputed swap simulation data for the slow chain.
///
/// Generated when a new slow chain block arrives, these precomputes are used to
/// quickly generate signals when the fast chain block arrives without needing to
/// re-simulate swaps. Contains swap simulations at various amounts and spot prices.
#[derive(Debug, Clone)]
pub struct Precomputes {
    /// Block height at which these precomputes were generated.
    pub block_height: u64,
    /// Spot prices for the primary trading pair (token_a/token_b).
    pub prices_a_b: SpotPrices,
    /// Sorted spot prices for each pool, used to find crossing pools.
    pub sorted_prices_a_b: Vec<(PoolId, f64)>,
    /// Precomputed swap simulations for each pool at various amounts.
    pub pool_sims: HashMap<state::PoolId, simulation::PoolSteps>,
    /// Protocol component metadata for each pool, needed for transaction encoding.
    pub pool_metadata: HashMap<state::PoolId, Arc<ProtocolComponent>>,
    /// ETH to USDC prices for gas cost calculation.
    pub prices_eth_usdc: Option<SpotPrices>,
    /// Token A to USDC prices for profit calculation.
    pub prices_a_usdc: Option<SpotPrices>,
    /// Token B to USDC prices for profit calculation.
    pub prices_b_usdc: Option<SpotPrices>,
    /// Base fee per gas unit from the block header.
    pub base_fee: u64,
}

/// Strategy configuration for cross-chain single-hop arbitrage.
///
/// Encapsulates all the parameters needed to detect and execute arbitrage opportunities
/// between two chains for a single token pair. The strategy monitors price differences
/// between slow chain and fast chain pools to identify profitable trades.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrossChainSingleHop {
    /// The token pair being traded on the slow chain.
    pub slow_pair: Pair,
    // TODO: docstring
    pub slow_usdc: Token,
    // TODO: docstring
    pub slow_eth: Token,
    // TODO: docstring
    pub slow_eth_usdc: Option<Pair>,
    // TODO: docstring
    pub slow_token_a_usdc: Option<Pair>,
    // TODO: docstring
    pub slow_token_b_usdc: Option<Pair>,
    // TODO: docstring
    pub slow_chain: Chain,
    // TODO: docstring
    pub fast_pair: Pair,
    // TODO: docstring
    pub fast_token_a_usdc: Option<Pair>,
    // TODO: docstring
    pub fast_token_b_usdc: Option<Pair>,
    // TODO: docstring
    pub fast_usdc: Token,
    // TODO: docstring
    pub fast_chain: Chain,
    // TODO: docstring
    pub slow_inventory: (BigUint, BigUint),
    // TODO: docstring
    pub fast_inventory: (BigUint, BigUint),
    // TODO: docstring
    pub binary_search_steps: usize,
    // TODO: docstring
    pub max_slippage_bps: u64,
    // TODO: docstring
    pub congestion_risk_discount_bps: u64,
}

impl CrossChainSingleHop {
    // TODO: maybe turn this func into async to parallelize the simulations?
    #[instrument(skip_all, fields(
        block.height = %state.pair_state.block_height,
        pair = %self.slow_pair,
        inventory = ?self.slow_inventory,
        chain = %self.slow_chain.name,
        with_unmodified_precomputes = %unmodified_precomputes.is_some(),
    ))]
    pub fn try_precompute(
        &self,
        state: BlockState,
        unmodified_precomputes: Option<Precomputes>,
    ) -> eyre::Result<Precomputes> {
        let block_height = state.pair_state.block_height;

        let mut pool_sims = HashMap::new();

        // reuse precomputes for unmodified pools
        if let Some(mut precomputes) = unmodified_precomputes {
            let unmodified_sims: HashMap<PoolId, simulation::PoolSteps> = state
                .pair_state
                .unmodified_pools
                .iter()
                .filter_map(|pool_id| {
                    let pool_sims = precomputes.pool_sims.remove(pool_id)?;
                    Some((pool_id.clone(), pool_sims))
                })
                .collect();

            pool_sims.extend(unmodified_sims);
        }

        // add simulation results for modified pools
        let precomputes = state
                    .pair_state
                    .modified_pools
                    .as_ref()
                    .iter()
                    .filter_map(|pool_id| state.pair_state.states.get(pool_id).map(|pool| (pool_id, pool)))
                    .filter_map(|(pool_id, state)| {
                        match simulation::PoolSteps::from_protocol_sim(&self.slow_pair, self.binary_search_steps, &self.slow_inventory, state.as_ref()) {
                            Ok(pool_sim) => Some((pool_id.clone(), pool_sim)),
                            Err(e) => {
                                error!(error = %e, pool.id = %pool_id, pair = %self.slow_pair, "precompute failed, skipping pool");
                                None
                            }
                        }
                    });
        pool_sims.extend(precomputes);

        // calculate a-b spot prices
        let sorted_prices_a_b = try_make_sorted_spot_prices(&state.pair_state, &self.slow_pair)
            .wrap_err("failed to simulate spot prices")?;
        let prices_a_b = SpotPrices::try_from_sorted_prices(
            &sorted_prices_a_b,
            block_height,
            self.slow_chain.clone(),
            self.slow_pair.clone(),
        )
        .wrap_err_with(|| {
            format!(
                "failed to simulate spot prices for {} on {}",
                self.slow_pair, self.slow_chain
            )
        })?;

        // calculate usdc spot prices
        let prices_a_usdc = if let (Some(token_a_usdc), Some(token_a_usdc_state)) =
            (&self.slow_token_a_usdc, &state.token_a_usdc_state)
        {
            let prices_a_usdc = SpotPrices::try_from_pair_state(
                token_a_usdc_state,
                token_a_usdc.clone(),
                self.slow_chain.clone(),
            )
            .wrap_err_with(|| {
                format!(
                    "failed to simulate spot prices for {} on {}",
                    token_a_usdc, self.slow_chain
                )
            })?;
            Some(prices_a_usdc)
        } else {
            trace!(pair = %self.slow_pair, "Skipping spot price simulation for token A == USDC");
            None
        };

        let prices_b_usdc = if let (Some(token_b_usdc), Some(token_b_usdc_state)) =
            (&self.slow_token_b_usdc, &state.token_b_usdc_state)
        {
            let prices_b_usdc = SpotPrices::try_from_pair_state(
                token_b_usdc_state,
                token_b_usdc.clone(),
                self.slow_chain.clone(),
            )
            .wrap_err_with(|| {
                format!(
                    "failed to simulate spot prices for {} on {}",
                    token_b_usdc, self.slow_chain
                )
            })?;
            Some(prices_b_usdc)
        } else {
            trace!(pair = %self.slow_pair, "Skipping spot price simulation for token B == USDC");
            None
        };

        let prices_eth_usdc = if let (Some(eth_usdc), Some(eth_usdc_state)) =
            (&self.slow_eth_usdc, &state.eth_usdc_state)
        {
            let prices_eth_usdc = SpotPrices::try_from_pair_state(
                eth_usdc_state,
                eth_usdc.clone(),
                self.slow_chain.clone(),
            )
            .wrap_err_with(|| {
                format!(
                    "failed to simulate spot prices for {} on {}",
                    eth_usdc, self.slow_chain
                )
            })?;
            Some(prices_eth_usdc)
        } else {
            trace!(pair = %self.slow_pair, "Skipping spot price simulation for ETH == USDC");
            None
        };

        Ok(Precomputes {
            block_height,
            prices_a_b,
            sorted_prices_a_b,
            pool_sims,
            pool_metadata: state.pair_state.metadata.clone(),
            prices_eth_usdc,
            prices_a_usdc,
            prices_b_usdc,
            base_fee: state.base_fee,
        })
    }

    #[instrument(skip_all, fields(
        slow.chain = %self.slow_chain,
        slow.pair = %self.slow_pair,
        slow.height = %precompute.block_height,
        fast.chain = %self.fast_chain,
        fast.pair = %self.fast_pair,
        fast.height = %fast_state.block_height
    ))]
    #[allow(clippy::too_many_arguments)]
    pub fn generate_signal(
        &self,
        precompute: &Precomputes,
        fast_state: PairState,
        fast_sorted_spot_prices: Vec<(PoolId, f64)>,
        fast_base_fee: u64,
    ) -> eyre::Result<signals::CrossChainSingleHop> {
        // 1. find the first pair of crossing pools from precompute & fast_state
        if fast_sorted_spot_prices.is_empty() {
            return Err(eyre::eyre!("No spot prices found for fast chain"));
        } else {
            trace!(
                min.pool_id = %fast_sorted_spot_prices[0].0,
                min.price = %fast_sorted_spot_prices[0].1,
                max.pool_id = %fast_sorted_spot_prices[fast_sorted_spot_prices.len() - 1].0,
                max.price = %fast_sorted_spot_prices[fast_sorted_spot_prices.len() - 1].1,
                chain = %self.fast_chain,
                "Computed spot prices for fast chain");
        }

        if let Some((slow_id, fast_id, direction)) =
            find_first_crossed_pools(&precompute.sorted_prices_a_b, &fast_sorted_spot_prices).map(
                |(slow_id, slow_price, fast_id, fast_price, spread)| {
                    let slow_direction = if spread > 0.0 {
                        Direction::AtoB
                    } else {
                        Direction::BtoA
                    };
                    trace!(
                        %slow_direction,
                        %spread,
                        %slow_price,
                        %fast_price,
                        %slow_id,
                        %fast_id,
                        "Found crossed pools"
                    );

                    (slow_id, fast_id, slow_direction)
                },
            )
        {
            match direction {
                Direction::AtoB => {
                    let signal = self
                        .find_optimal_signal(
                            &precompute.pool_sims[&slow_id].a_to_b,
                            precompute.pool_metadata[&slow_id].clone(),
                            &slow_id,
                            precompute.block_height,
                            &precompute.prices_a_b,
                            &precompute.prices_a_usdc,
                            &precompute.prices_b_usdc,
                            &precompute.prices_eth_usdc,
                            precompute.base_fee,
                            fast_state.states[&fast_id].as_ref(),
                            fast_state.metadata[&fast_id].clone(),
                            &fast_id,
                            fast_state.block_height,
                            &self.fast_inventory.1,
                            fast_base_fee,
                        )
                        .wrap_err("no optimal signal for A->B (slow) and B->A (fast)")?;
                    trace!(
                        slow_sim = %signal.slow_swap_sim,
                        fast_sim = %signal.fast_swap_sim,
                        signal.expected_profit = %signal.expected_profit,
                        "found optimal swap for A->B (slow) and B->A (fast)"
                    );
                    Ok(signal)
                }
                Direction::BtoA => {
                    let signal = self
                        .find_optimal_signal(
                            &precompute.pool_sims[&slow_id].b_to_a,
                            precompute.pool_metadata[&slow_id].clone(),
                            &slow_id,
                            precompute.block_height,
                            &precompute.prices_a_b,
                            &precompute.prices_a_usdc,
                            &precompute.prices_b_usdc,
                            &precompute.prices_eth_usdc,
                            precompute.base_fee,
                            fast_state.states[&fast_id].as_ref(),
                            fast_state.metadata[&fast_id].clone(),
                            &fast_id,
                            fast_state.block_height,
                            &self.fast_inventory.0,
                            fast_base_fee,
                        )
                        .wrap_err("no optimal signal for B->A (slow) and A->B (fast)")?;
                    trace!(
                        slow_sim = %signal.slow_swap_sim,
                        fast_sim = %signal.fast_swap_sim,
                        signal.expected_profit = %signal.expected_profit,
                        "found optimal swap for B->A (slow) and A->B (fast)"
                    );
                    Ok(signal)
                }
            }
        } else {
            Err(eyre!(
                "no crossing pools found for A->B (slow) and B->A (fast)"
            ))
        }
    }

    /// Finds the optimal swap for a given direction.
    ///
    /// Uses a binary search over the slow chain simulations created in the precompute step.
    /// This assumes the profit function behaves "unimodally", i.e. has a single peak, in terms of
    /// amount_in -> profit.
    ///
    /// At each step, the search compares the profit at `mid` to `mid + 1`.
    /// If `mid` < `mid + 1`, the search continues in the right half of the array.
    /// If `mid` >= `mid + 1`, the search continues in the left half of the array.
    ///
    /// Each step uses a precomputed slow chain `Swap` and the fast chain's `ProtocolSim` to create
    /// the fast chain's `Swap`, and a candidate `signals::CrossChainSingleHop`. The signals'
    /// expected profits are compared to find the optimal signal.
    #[allow(clippy::too_many_arguments)]
    fn find_optimal_signal(
        &self,
        // TODO: have an abstraction around slow = (height, pool_id, sims) and fast = (height, pool_id, protocol_sim, inventory)
        slow_sims: &[Swap],
        slow_protocol_component: Arc<ProtocolComponent>,
        slow_pool_id: &PoolId,
        slow_height: u64,
        slow_prices_a_b: &SpotPrices,
        slow_prices_a_usdc: &Option<SpotPrices>,
        slow_prices_b_usdc: &Option<SpotPrices>,
        slow_prices_eth_usdc: &Option<SpotPrices>,
        slow_base_fee: u64,
        fast_state: &dyn ProtocolSim,
        fast_protocol_component: Arc<ProtocolComponent>,
        fast_pool_id: &PoolId,
        fast_height: u64,
        fast_inventory: &BigUint,
        fast_base_fee: u64,
    ) -> eyre::Result<signals::CrossChainSingleHop> {
        // Binary search to find the optimal signal. The search assumes the profit function is
        // unimodal (has a single peak) over the slow_sims array. At each step, we compare the
        // profit at `mid` vs `mid + 1`:
        // - If mid < mid+1, the peak is to the right, so we search right (left = mid + 1)
        // - If mid >= mid+1, the peak is at mid or to the left, so we search left (right = mid)
        // We track the best signal seen during the search to avoid recomputing at the end.
        let (mut left, mut right) = (0, slow_sims.len() - 1);

        // Handle single-element case where the loop won't run (left == right)
        if left == right {
            let signal = self
                .try_signal_from_precompute(
                    slow_sims[left].clone(),
                    slow_protocol_component,
                    slow_pool_id,
                    slow_height,
                    slow_prices_a_b,
                    slow_prices_a_usdc,
                    slow_prices_b_usdc,
                    slow_prices_eth_usdc,
                    slow_base_fee,
                    fast_state,
                    fast_protocol_component,
                    fast_pool_id,
                    fast_height,
                    fast_inventory,
                    fast_base_fee,
                )
                .wrap_err("failed to create signal from single precompute")?;
            return Ok(signal);
        }

        let mut best_signal = None;

        // Each iteration compares profit at mid vs mid+1 and narrows the search range.
        // The winning signal is saved to best_signal.
        // Failed signals are treated as having zero profit, so valid signals are preferred.
        while left < right {
            let mid = left + (right - left) / 2;
            let mid_signal = self
                .try_signal_from_precompute(
                    slow_sims[mid].clone(),
                    slow_protocol_component.clone(),
                    slow_pool_id,
                    slow_height,
                    slow_prices_a_b,
                    slow_prices_a_usdc,
                    slow_prices_b_usdc,
                    slow_prices_eth_usdc,
                    slow_base_fee,
                    fast_state,
                    fast_protocol_component.clone(),
                    fast_pool_id,
                    fast_height,
                    fast_inventory,
                    fast_base_fee,
                )
                .inspect_err(|err| {
                    trace!(index = mid, %err, "failed to create mid signal");
                })
                .ok();
            let mid_profit = mid_signal
                .as_ref()
                .map(|s| &s.expected_profit.min_total_amount_usdc);

            let next = mid + 1;
            let next_signal = self
                .try_signal_from_precompute(
                    slow_sims[next].clone(),
                    slow_protocol_component.clone(),
                    slow_pool_id,
                    slow_height,
                    slow_prices_a_b,
                    slow_prices_a_usdc,
                    slow_prices_b_usdc,
                    slow_prices_eth_usdc,
                    slow_base_fee,
                    fast_state,
                    fast_protocol_component.clone(),
                    fast_pool_id,
                    fast_height,
                    fast_inventory,
                    fast_base_fee,
                )
                .inspect_err(|err| {
                    trace!(index = next, %err, "failed to create next signal");
                })
                .ok();
            let next_profit = next_signal
                .as_ref()
                .map(|s| &s.expected_profit.min_total_amount_usdc);

            // Compare Option<&BigUint> directly: None < Some(&signal.expected_profit.min_total_amount_usdc), so failed signals are always "less profitable"
            if mid_profit < next_profit {
                trace!(index = mid, left = %left, right = %right, mid_profit = ?mid_profit, next_profit = ?next_profit, "mid+1 has higher profit, searching right");
                left = mid + 1;
                best_signal = next_signal;
            } else {
                trace!(index = mid, left = %left, right = %right, mid_profit = ?mid_profit, next_profit = ?next_profit, "mid has equal or higher profit, searching left");
                right = mid;
                best_signal = mid_signal;
            }
        }

        match best_signal {
            Some(signal) => {
                trace!(index = %left, found_signal = %signal, "search complete");
                Ok(signal)
            }
            None => {
                trace!("no valid signal found during search");
                Err(eyre!("all signals failed to create during search"))
            }
        }
    }

    /// This creates the fast leg of the arbitrage out of the precompute slow leg.
    fn swap_from_precompute(
        &self,
        precompute: simulation::Swap,
        fast_state: &dyn ProtocolSim,
        fast_inventory: &BigUint,
        max_slippage_bps: u64,
    ) -> eyre::Result<simulation::Swap> {
        let amount_in = bps_discount(&precompute.amount_out, max_slippage_bps);

        if fast_inventory < &amount_in {
            return Err(eyre::eyre!("fast inventory is insufficient"));
        }

        let (token_in, token_out) = {
            if precompute.token_in == *self.slow_pair.token_a() {
                // if slow is A->B then fast is B->A
                (self.fast_pair.token_b(), self.fast_pair.token_a())
            } else {
                // if slow is B->A then fast is A->B
                (self.fast_pair.token_a(), self.fast_pair.token_b())
            }
        };

        Swap::from_protocol_sim(&amount_in, token_in, token_out, fast_state)
            .wrap_err("swap simulation failed")
    }

    #[allow(clippy::too_many_arguments)]
    fn try_signal_from_precompute(
        &self,
        slow_sim: Swap,
        slow_protocol_component: Arc<ProtocolComponent>,
        slow_pool_id: &PoolId,
        slow_height: u64,
        slow_prices_a_b: &SpotPrices,
        slow_prices_a_usdc: &Option<SpotPrices>,
        slow_prices_b_usdc: &Option<SpotPrices>,
        slow_prices_eth_usdc: &Option<SpotPrices>,
        slow_base_fee: u64,
        fast_state: &dyn ProtocolSim,
        fast_protocol_component: Arc<ProtocolComponent>,
        fast_pool_id: &PoolId,
        fast_height: u64,
        fast_inventory: &BigUint,
        fast_base_fee: u64,
    ) -> eyre::Result<signals::CrossChainSingleHop> {
        let fast_sim = match self.swap_from_precompute(
            slow_sim.clone(),
            fast_state,
            fast_inventory,
            self.max_slippage_bps,
        ) {
            Ok(swap) => swap,
            Err(err) => {
                return Err(eyre!(
                    "failed to simulate fast swap from {slow_sim} with err: {err}"
                ));
            }
        };

        signals::CrossChainSingleHop::try_from_simulations(
            &self.slow_chain,
            &self.slow_pair,
            slow_protocol_component,
            slow_pool_id,
            slow_height,
            slow_sim.clone(),
            slow_prices_a_b,
            slow_prices_a_usdc,
            slow_prices_b_usdc,
            slow_prices_eth_usdc,
            slow_base_fee,
            &self.fast_chain,
            &self.fast_pair,
            fast_protocol_component,
            fast_pool_id,
            fast_height,
            fast_sim.clone(),
            self.max_slippage_bps,
            self.congestion_risk_discount_bps,
            fast_base_fee,
        )
        .map_err(|err| {
            trace!(%slow_sim, %fast_sim, %err, "‼️ failed to make signal");
            err
        })
    }

    pub fn token_a_symbol(&self) -> &str {
        &self.slow_pair.token_a().symbol
    }

    pub fn token_b_symbol(&self) -> &str {
        &self.slow_pair.token_b().symbol
    }
}

/// Finds the pair of pools with the biggest difference in spot prices based
/// on the provided direction. The direction denotes the trade direction on the
/// slow chain.
///
/// `sorted_slow_prices` contain the A -> B prices on the slow chain, sorted ascending.
/// `sorted_fast_prices` contain the A -> B prices on the fast chain, sorted ascending.
///
/// # Returns
/// A tuple `(slow_id, slow_price, fast_id, fast_price)` corresponding to the
/// pools with the largest spread between them.
#[instrument]
fn find_first_crossed_pools(
    sorted_slow_prices: &[(state::PoolId, f64)],
    sorted_fast_prices: &[(state::PoolId, f64)],
) -> Option<(state::PoolId, f64, state::PoolId, f64, f64)> {
    if sorted_slow_prices.is_empty() || sorted_fast_prices.is_empty() {
        return None;
    }

    let (slow_min_id, slow_min) = &sorted_slow_prices[0];
    let (slow_max_id, slow_max) = &sorted_slow_prices[sorted_slow_prices.len() - 1];
    let (fast_min_id, fast_min) = &sorted_fast_prices[0];
    let (fast_max_id, fast_max) = &sorted_fast_prices[sorted_fast_prices.len() - 1];

    // Two possible "extreme" spreads
    let short_slow_long_fast_spread = slow_max - fast_min;
    let long_slow_short_fast_spread = slow_min - fast_max;

    if short_slow_long_fast_spread.abs() >= long_slow_short_fast_spread.abs() {
        trace!(%slow_max_id, %slow_max, %fast_min_id, %fast_min, %short_slow_long_fast_spread, "max spread slow_max - fast_min");
        Some((
            slow_max_id.clone(),
            *slow_max,
            fast_min_id.clone(),
            *fast_min,
            short_slow_long_fast_spread,
        ))
    } else {
        trace!(%slow_min_id, %slow_min, %fast_max_id, %fast_max, %long_slow_short_fast_spread, "max spread fast_max - slow_min");
        Some((
            slow_min_id.clone(),
            *slow_min,
            fast_max_id.clone(),
            *fast_max,
            long_slow_short_fast_spread,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        chain::Chain,
        signals::ExpectedProfit,
        state::{self, pair::PairState},
        strategy::{self, CrossChainSingleHop},
    };
    use sqlx::types::chrono::NaiveDateTime;
    use std::{
        collections::{HashMap, HashSet},
        str::FromStr as _,
        sync::{Arc, OnceLock},
    };
    use tracing_subscriber::EnvFilter;
    use tycho_common::simulation::protocol_sim::ProtocolSim;
    use tycho_simulation::tycho_common::{self, models::token::Token};

    static TELEMETRY_INIT: OnceLock<()> = OnceLock::new();

    // zero2one order: PEPE < TEST6 < USDC_BASE < WETH < USDC_MAINNET
    // Pairs:
    //  - PEPE/WETH
    //  - PEPE/USDC_BASE
    //  - PEPE/USDC_MAINNET
    //
    //  - TEST6/WETH
    //  - TEST6/USDC_BASE
    //  - TEST6/USDC_MAINNET
    //
    //  - USDC_BASE/WETH
    //  - WETH/USDC_MAINNET
    const PEPE_ADDRESS: &str = "0x0000000000000000000000000000000000000001";
    const TEST6_ADDRESS: &str = "0x0000000000000000000000000000000000000002";
    const WETH_ADDRESS: &str = "0x0000000000000000000000000000000000000100";
    const USDC_BASE_ADDRESS: &str = "0x0000000000000000000000000000000000000010";
    const USDC_MAINNET_ADDRESS: &str = "0x0000000000000000000000000000000000000110";

    fn init_telemetry() {
        TELEMETRY_INIT.get_or_init(|| {
            color_eyre::install().unwrap();

            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    EnvFilter::from_default_env()
                        .add_directive(
                            "tycho_client=warn"
                                .parse()
                                .expect("well-formed tracing directive should parse"),
                        )
                        .add_directive(
                            "tycho_simulation=warn"
                                .parse()
                                .expect("well-formed tracing directive should parse"),
                        ),
                )
                .with_thread_names(true)
                .pretty()
                .with_line_number(true)
                .with_test_writer()
                .try_init();
        });
    }

    fn make_18_dec_token(chain: tycho_common::models::Chain, symbol: &str, address: &str) -> Token {
        Token::new(
            // 0x0..00 address for uniswap zero2one pool order
            &tycho_common::Bytes::from_str(address).expect("valid address"),
            symbol,
            18,
            1000,
            &[Some(1000u64)],
            chain,
            100,
        )
    }

    fn make_6_dec_token(chain: tycho_common::models::Chain, symbol: &str, address: &str) -> Token {
        Token::new(
            &tycho_common::Bytes::from_str(address).expect("valid address"),
            symbol,
            6,
            1000,
            &[Some(1000u64)],
            chain,
            100,
        )
    }

    fn make_mainnet_pepe() -> Token {
        make_18_dec_token(tycho_common::models::Chain::Ethereum, "PEPE", PEPE_ADDRESS)
    }

    fn make_base_pepe() -> Token {
        make_18_dec_token(tycho_common::models::Chain::Base, "PEPE", PEPE_ADDRESS)
    }

    fn make_mainnet_test_6_token() -> Token {
        make_6_dec_token(
            tycho_common::models::Chain::Ethereum,
            "TEST6",
            TEST6_ADDRESS,
        )
    }

    fn make_base_test_6_token() -> Token {
        make_6_dec_token(tycho_common::models::Chain::Base, "TEST6", TEST6_ADDRESS)
    }

    fn make_mainnet_weth() -> Token {
        make_18_dec_token(tycho_common::models::Chain::Ethereum, "WETH", WETH_ADDRESS)
    }

    fn make_base_weth() -> Token {
        make_18_dec_token(tycho_common::models::Chain::Base, "WETH", WETH_ADDRESS)
    }

    fn make_mainnet_usdc() -> Token {
        make_6_dec_token(
            tycho_common::models::Chain::Ethereum,
            "USDC",
            USDC_MAINNET_ADDRESS,
        )
    }

    fn make_base_usdc() -> Token {
        make_6_dec_token(tycho_common::models::Chain::Base, "USDC", USDC_BASE_ADDRESS)
    }

    fn scale_by_decimals(amount: &BigUint, decimals: u32) -> BigUint {
        amount * BigUint::from(10u64).pow(decimals)
    }

    fn make_univ2_protocol_sim(reserve_0: &BigUint, reserve_1: &BigUint) -> Arc<dyn ProtocolSim> {
        use std::str::FromStr;
        use tycho_simulation::evm::protocol::uniswap_v2::state::UniswapV2State;

        let reserve_0_u256 = alloy::primitives::U256::from_str(&reserve_0.to_string()).unwrap();
        let reserve_1_u256 = alloy::primitives::U256::from_str(&reserve_1.to_string()).unwrap();

        Arc::new(UniswapV2State::new(reserve_0_u256, reserve_1_u256))
    }

    fn make_single_univ2_pair_state(
        pair: &Pair,
        block_height: u64,
        pool_id: &str,
        reserve_a: u64,
        reserve_b: u64,
        chain: tycho_common::models::Chain,
    ) -> PairState {
        let (reserve0, reserve1) = {
            let reserve_a = scale_by_decimals(&BigUint::from(reserve_a), pair.token_a().decimals);
            let reserve_b = scale_by_decimals(&BigUint::from(reserve_b), pair.token_b().decimals);

            if pair.token_a().address < pair.token_b().address {
                (reserve_a, reserve_b)
            } else {
                (reserve_b, reserve_a)
            }
        };

        PairState {
            states: HashMap::from([(
                state::PoolId::from(pool_id),
                make_univ2_protocol_sim(&reserve0, &reserve1),
            )]),
            block_height,
            modified_pools: Arc::new(HashSet::from([state::PoolId::from(pool_id)])),
            unmodified_pools: Arc::new(HashSet::new()),
            metadata: HashMap::from([(
                state::PoolId::from(pool_id),
                Arc::new(ProtocolComponent::new(
                    pool_id.as_bytes().into(),
                    String::from("univ2"),
                    String::from("univ2"),
                    chain,
                    vec![pair.token_a().clone(), pair.token_b().clone()],
                    vec![pool_id.as_bytes().into()],
                    HashMap::new(),
                    tycho_common::Bytes::from_str("0123").unwrap(),
                    NaiveDateTime::default(),
                )),
            )]),
        }
    }

    fn simulate_swap_for_pool_id(
        pool_id: &str,
        amount_in: BigUint,
        token_in: &Token,
        token_out: &Token,
        state: PairState,
    ) -> Swap {
        let pool_id = state::PoolId::from(pool_id);
        let pool_state = state.states.get(&pool_id).expect("pool state not found");
        Swap::from_protocol_sim(&amount_in, token_in, token_out, pool_state.as_ref()).unwrap()
    }

    /// Creates a BlockState for the slow chain with constant USDC prices.
    ///
    /// Constant USDC prices for same-decimals strategy:
    /// - PEPE->USDC = 0.1 (10k PEPE : 1k USDC)
    /// - WETH->USDC = 3000 (1 WETH : 3000 USDC)
    ///
    /// Pool IDs created:
    /// - "slow_main" - main A-B pair
    /// - "slow_a_usdc" - token A to USDC pair
    /// - "slow_b_usdc" - token B to USDC pair
    fn make_slow_block_state_same_decimals(
        strategy: &CrossChainSingleHop,
        block_height: u64,
        main_reserve_a: u64,
        main_reserve_b: u64,
    ) -> state::block::BlockState {
        let pair_state = make_single_univ2_pair_state(
            &strategy.slow_pair,
            block_height,
            "slow_main",
            main_reserve_a,
            main_reserve_b,
            strategy.slow_chain.name,
        );

        // Constant USDC prices: PEPE->USDC = 0.1 (10k PEPE : 1k USDC)
        let token_a_usdc_state = if let Some(slow_token_a_usdc) = &strategy.slow_token_a_usdc {
            let token_a_usdc_state = make_single_univ2_pair_state(
                slow_token_a_usdc,
                block_height,
                "slow_a_usdc",
                10_000,
                1_000,
                strategy.slow_chain.name,
            );
            Some(token_a_usdc_state)
        } else {
            None
        };

        // Constant USDC prices: WETH->USDC = 3000 (1 WETH : 3000 USDC)
        let token_b_usdc_state = if let Some(slow_token_b_usdc) = &strategy.slow_token_b_usdc {
            let token_b_usdc_state = make_single_univ2_pair_state(
                slow_token_b_usdc,
                block_height,
                "slow_b_usdc",
                1,
                3_000,
                strategy.slow_chain.name,
            );
            Some(token_b_usdc_state)
        } else {
            None
        };

        // Constant ETH gas price: ETH->USDC = 3000 (1 ETH : 3000 USDC)
        let eth_usdc_state = if let Some(slow_token_b_usdc) = &strategy.slow_token_b_usdc {
            let eth_usdc_state = make_single_univ2_pair_state(
                slow_token_b_usdc,
                block_height,
                "slow_eth_usdc",
                1,
                3_000,
                strategy.slow_chain.name,
            );
            Some(eth_usdc_state)
        } else {
            None
        };

        state::block::BlockState {
            pair_state,
            token_a_usdc_state,
            token_a_balance: BigUint::from(0u64),
            token_b_usdc_state,
            token_b_balance: BigUint::from(0u64),
            eth_usdc_state,
            base_fee: 0u64, // TODO: non-zero base fee
        }
    }

    /// Creates a block state for a fast chain with same token decimals.
    fn make_fast_block_same_decimals(
        strategy: &CrossChainSingleHop,
        block_height: u64,
        main_reserve_a: u64,
        main_reserve_b: u64,
    ) -> state::block::BlockState {
        let pair_state = make_single_univ2_pair_state(
            &strategy.fast_pair,
            block_height,
            "fast_main",
            main_reserve_a,
            main_reserve_b,
            strategy.fast_chain.name,
        );

        // Constant USDC prices: PEPE->USDC = 0.1 (10k PEPE : 1k USDC)
        let token_a_usdc_state = if let Some(slow_token_a_usdc) = &strategy.fast_token_a_usdc {
            let pair_state = make_single_univ2_pair_state(
                slow_token_a_usdc,
                block_height,
                "fast_a_usdc",
                10_000,
                1_000,
                strategy.fast_chain.name,
            );
            Some(pair_state)
        } else {
            None
        };

        // Constant USDC prices: WETH->USDC = 3000 (1000 WETH : 3m USDC)
        let token_b_usdc_state = if let Some(fast_token_b_usdc) = &strategy.fast_token_b_usdc {
            let pair_state = make_single_univ2_pair_state(
                fast_token_b_usdc,
                block_height,
                "fast_b_usdc",
                1_000,
                3_000_000,
                strategy.fast_chain.name,
            );
            Some(pair_state)
        } else {
            None
        };

        // Constant ETH gas price: ETH->USDC = 3000 (1 ETH : 3000 USDC)
        let eth_usdc_state = if let Some(fast_token_b_usdc) = &strategy.fast_token_b_usdc {
            let eth_usdc_state = make_single_univ2_pair_state(
                fast_token_b_usdc,
                block_height,
                "fast_eth_usdc",
                1,
                3_000,
                strategy.fast_chain.name,
            );
            Some(eth_usdc_state)
        } else {
            None
        };

        state::block::BlockState {
            pair_state,
            token_a_usdc_state,
            token_a_balance: BigUint::from(0u64),
            token_b_usdc_state,
            token_b_balance: BigUint::from(0u64),
            eth_usdc_state,
            base_fee: 0u64, // TODO: non-zero base fee
        }
    }

    /// Creates a BlockState for the slow chain with different decimals and constant USDC prices.
    ///
    /// Constant USDC prices for different-decimals strategy:
    /// - PEPE (6 decimals)->USDC = 0.2 (5k PEPE : 1k USDC)
    /// - WETH (18 decimals)->USDC = 3000 (1 WETH : 3000 USDC)
    ///
    /// Pool IDs created:
    /// - "slow_main" - main A-B pair
    /// - "slow_a_usdc" - token A to USDC pair
    /// - "slow_b_usdc" - token B to USDC pair
    fn make_slow_block_state_different_decimals(
        strategy: &CrossChainSingleHop,
        block_height: u64,
        main_reserve_a: u64,
        main_reserve_b: u64,
    ) -> state::block::BlockState {
        let pair_state = make_single_univ2_pair_state(
            &strategy.slow_pair,
            block_height,
            "slow_main",
            main_reserve_a,
            main_reserve_b,
            strategy.slow_chain.name,
        );

        // Constant USDC prices: WETH->USDC = 3000 (1000 WETH : 3m USDC)
        let token_a_usdc_state = if let Some(token_a_usdc_pair) = &strategy.slow_token_a_usdc {
            let token_a_usdc_state = make_single_univ2_pair_state(
                token_a_usdc_pair,
                block_height,
                "slow_a_usdc",
                1_000,
                3_000_000,
                strategy.slow_chain.name,
            );
            Some(token_a_usdc_state)
        } else {
            None
        };

        // Constant USDC prices: TEST->USDC = 0.2 (5m TEST : 1m USDC)
        let token_b_usdc_state = if let Some(token_b_usdc_pair) = &strategy.slow_token_b_usdc {
            let token_b_usdc_state = make_single_univ2_pair_state(
                token_b_usdc_pair,
                block_height,
                "slow_b_usdc",
                5_000_000,
                1_000_000,
                strategy.slow_chain.name,
            );
            Some(token_b_usdc_state)
        } else {
            None
        };

        // Constant ETH gas price: ETH->USDC = 3000 (1 ETH : 3000 USDC)
        let eth_usdc_state = if let Some(token_b_usdc_pair) = &strategy.slow_token_b_usdc {
            let eth_usdc_state = make_single_univ2_pair_state(
                token_b_usdc_pair,
                block_height,
                "slow_eth_usdc",
                1,
                3_000,
                strategy.slow_chain.name,
            );
            Some(eth_usdc_state)
        } else {
            None
        };

        state::block::BlockState {
            pair_state,
            token_a_usdc_state,
            token_a_balance: BigUint::from(0u64),
            token_b_usdc_state,
            token_b_balance: BigUint::from(0u64),
            eth_usdc_state,
            base_fee: 0u64, // TODO: non-zero base fee
        }
    }

    /// Creates a block state for a fast chain with different token decimals.
    fn make_fast_block_different_decimals(
        strategy: &CrossChainSingleHop,
        block_height: u64,
        main_reserve_a: u64,
        main_reserve_b: u64,
    ) -> state::block::BlockState {
        let pair_state = make_single_univ2_pair_state(
            &strategy.fast_pair,
            block_height,
            "fast_main",
            main_reserve_a,
            main_reserve_b,
            strategy.fast_chain.name,
        );

        // Constant USDC prices: WETH->USDC = 3000 (1 WETH : 3000 USDC)
        let token_a_usdc_state = if let Some(pair) = &strategy.fast_token_a_usdc {
            let state = make_single_univ2_pair_state(
                pair,
                block_height,
                "fast_a_usdc",
                1,
                3_000,
                strategy.slow_chain.name,
            );
            Some(state)
        } else {
            None
        };

        // Constant USDC prices: TEST->USDC = 0.2 (5k TEST : 1k USDC)
        let token_b_usdc_state = if let Some(pair) = &strategy.fast_token_b_usdc {
            let state = make_single_univ2_pair_state(
                pair,
                block_height,
                "fast_b_usdc",
                5_000,
                1_000,
                strategy.slow_chain.name,
            );
            Some(state)
        } else {
            None
        };

        // Constant ETH gas price: ETH->USDC = 3000 (1 ETH : 3000 USDC)
        let eth_usdc_state = if let Some(pair) = &strategy.fast_token_b_usdc {
            let state = make_single_univ2_pair_state(
                pair,
                block_height,
                "fast_eth_usdc",
                1,
                3_000,
                strategy.slow_chain.name,
            );
            Some(state)
        } else {
            None
        };

        state::block::BlockState {
            pair_state,
            token_a_usdc_state,
            token_a_balance: BigUint::from(0u64),
            token_b_usdc_state,
            token_b_balance: BigUint::from(0u64),
            eth_usdc_state,
            base_fee: 0u64, // TODO: non-zero base fee
        }
    }

    /// Creates a strategy that uses the same number of decimals for all tokens.
    ///
    /// Token A = PEPE, Slow Address = 0x000, Fast Address = 0x000
    /// Token B = WETH, Slow Address = 0x100, Fast Address = 0x100
    /// Pair = PEPE/WETH, Slow zero2one = PEPE/WETH, Fast zero2one = PEPE/WETH
    ///
    /// USDC Slow Address = 0x110, Fast Address = 0x010
    /// Slow zero2one: Token A -> PEPE/USDC, Token B -> WETH/USDC
    /// Fast zero2one: Token A -> PEPE/USDC, Token B -> USDC/WETH
    fn make_same_decimals_strategy() -> Arc<strategy::CrossChainSingleHop> {
        init_telemetry();

        // WETH = 0x100, PEPE = 0x000, USDC = 0x110
        let slow_chain = Chain::eth_mainnet();
        // Pair = PEPE/WETH, zero2one = PEPE/WETH
        let slow_pair = Pair::new(make_mainnet_pepe(), make_mainnet_weth());

        // zero2one = PEPE/USDC
        let slow_token_a_usdc = Pair::new(make_mainnet_pepe(), make_mainnet_usdc());
        // zero2one = WETH/USDC
        let slow_token_b_usdc = Pair::new(make_mainnet_weth(), make_mainnet_usdc());

        // zero2one = WETH/USDC
        let slow_eth_usdc = Pair::new(make_mainnet_weth(), make_mainnet_usdc());

        let available_inventory_slow = (
            scale_by_decimals(&BigUint::from(50u64), slow_pair.token_a().decimals),
            scale_by_decimals(&BigUint::from(100u64), slow_pair.token_b().decimals),
        );

        // WETH = 0x100, PEPE = 0x000, USDC = 0x110
        let fast_chain = Chain::base_mainnet();
        // Pair = PEPE/WETH, zero2one = PEPE/WETH
        let fast_pair = Pair::new(make_base_pepe(), make_base_weth());

        // zero2one = PEPE/USDC
        let fast_token_a_usdc = Pair::new(make_base_pepe(), make_base_usdc());
        // zero2one = USDC/WETH
        let fast_token_b_usdc = Pair::new(make_base_weth(), make_base_usdc());

        let available_inventory_fast = (
            scale_by_decimals(&BigUint::from(200u64), fast_pair.token_a().decimals),
            scale_by_decimals(&BigUint::from(150u64), fast_pair.token_b().decimals),
        );

        Arc::new(CrossChainSingleHop {
            slow_chain,
            slow_usdc: make_mainnet_usdc(),
            slow_eth: make_mainnet_weth(),
            slow_pair,
            slow_inventory: available_inventory_slow,
            fast_chain,
            fast_usdc: make_base_usdc(),
            fast_pair,
            fast_inventory: available_inventory_fast,
            max_slippage_bps: 25, // 0.25%
            congestion_risk_discount_bps: 25,
            binary_search_steps: 16,
            slow_eth_usdc: Some(slow_eth_usdc),
            slow_token_a_usdc: Some(slow_token_a_usdc),
            slow_token_b_usdc: Some(slow_token_b_usdc),
            fast_token_a_usdc: Some(fast_token_a_usdc),
            fast_token_b_usdc: Some(fast_token_b_usdc),
        })
    }

    /// Same as `make_same_decimals_strategy` but with `binary_search_steps = 1`.
    fn make_same_decimals_strategy_one_step() -> Arc<strategy::CrossChainSingleHop> {
        let base = make_same_decimals_strategy();
        Arc::new(CrossChainSingleHop {
            binary_search_steps: 1,
            ..(*base).clone()
        })
    }

    /// Creates a strategy that uses different number of decimals for tokens.
    ///
    /// Token A = TEST6 (6 decimals), Slow Address = 0x002, Fast Address = 0x002
    /// Token B = WETH (18 decimals), Slow Address = 0x100, Fast Address = 0x100
    /// Pair = TEST6/WETH, Slow zero2one = TEST6/WETH, Fast zero2one = TEST6/WETH
    ///
    /// USDC Slow Address = 0x110, Fast Address = 0x010
    /// USDC Slow zero2one: Token A -> TEST6/USDC, Token B -> WETH/USDC
    /// USDC Fast zero2one: Token A -> TEST6/USDC, Token B -> USDC/WETH
    fn make_different_decimals_strategy() -> Arc<strategy::CrossChainSingleHop> {
        init_telemetry();

        // WETH = 0x100, TEST6 = 0x002, USDC = 0x110
        let slow_chain = Chain::eth_mainnet();
        // Pair = TEST6/WETH, zero2one = TEST6/WETH
        let slow_pair = Pair::new(make_mainnet_weth(), make_mainnet_test_6_token());

        // zero2one = TEST6/USDC
        let slow_token_a_usdc = Pair::new(make_mainnet_test_6_token(), make_mainnet_usdc());
        // zero2one = WETH/USDC
        let slow_token_b_usdc = Pair::new(make_mainnet_weth(), make_mainnet_usdc());

        // zero2one = WETH/USDC
        let slow_eth_usdc = Pair::new(make_mainnet_weth(), make_mainnet_usdc());

        let available_inventory_slow = (
            scale_by_decimals(&BigUint::from(50_000u64), slow_pair.token_a().decimals),
            scale_by_decimals(&BigUint::from(100u64), slow_pair.token_b().decimals),
        );

        // WETH = 0x100, TEST6 = 0x002, USDC = 0x010
        let fast_chain = Chain::base_mainnet();
        // Pair = TEST6/WETH, zero2one = TEST6/WETH
        let fast_pair = Pair::new(make_base_weth(), make_base_test_6_token());

        // zero2one = TEST6/USDC
        let fast_token_a_usdc = Pair::new(make_base_test_6_token(), make_base_usdc());
        // zero2one = USDC/WETH
        let fast_token_b_usdc = Pair::new(make_base_weth(), make_base_usdc());

        let available_inventory_fast = (
            scale_by_decimals(&BigUint::from(200_000u64), fast_pair.token_a().decimals),
            scale_by_decimals(&BigUint::from(500u64), fast_pair.token_b().decimals),
        );

        Arc::new(CrossChainSingleHop {
            slow_chain,
            slow_usdc: make_mainnet_usdc(),
            slow_eth: make_mainnet_weth(),
            slow_pair,
            slow_inventory: available_inventory_slow,
            fast_chain,
            fast_usdc: make_base_usdc(),
            fast_pair,
            fast_inventory: available_inventory_fast,
            max_slippage_bps: 25, // 0.25%
            congestion_risk_discount_bps: 25,
            binary_search_steps: 16,
            slow_eth_usdc: Some(slow_eth_usdc),
            slow_token_a_usdc: Some(slow_token_a_usdc),
            slow_token_b_usdc: Some(slow_token_b_usdc),
            fast_token_a_usdc: Some(fast_token_a_usdc),
            fast_token_b_usdc: Some(fast_token_b_usdc),
        })
    }

    #[test]
    fn precompute_same_decimals() {
        // Arrange
        let strategy = make_same_decimals_strategy();

        // Spot price: PEPE->WETH = 0.001 (1m PEPE : 1k WETH)
        let slow_state = make_slow_block_state_same_decimals(&strategy, 0, 1_000_000, 1_000);

        // Act
        let precompute = strategy
            .try_precompute(slow_state, None)
            .expect("precompute shouldn't fail");
        assert_eq!(precompute.block_height, 0);

        // Assert
        // correct spot prices
        assert_eq!(
            precompute.sorted_prices_a_b[0],
            (state::PoolId::from("slow_main"), "0.001".parse().unwrap())
        );

        // assert that only one pool is simulated
        assert_eq!(precompute.pool_sims.len(), 1);
        assert_eq!(
            precompute.pool_sims[&state::PoolId::from("slow_main")]
                .a_to_b
                .len(),
            strategy.binary_search_steps
        );
        assert_eq!(
            precompute.pool_sims[&state::PoolId::from("slow_main")]
                .b_to_a
                .len(),
            strategy.binary_search_steps
        );

        // check valid first and last step inputs
        let first_a_to_b = &precompute.pool_sims[&state::PoolId::from("slow_main")].a_to_b[0];
        assert_eq!(
            first_a_to_b.amount_in,
            BigUint::from_str("3125000000000000000").unwrap()
        );

        let first_b_to_a = &precompute.pool_sims[&state::PoolId::from("slow_main")].b_to_a[0];
        assert_eq!(
            first_b_to_a.amount_in,
            BigUint::from_str("6250000000000000000").unwrap()
        );

        // check valid last step inputs
        let last_amount_in_a = &precompute.pool_sims[&state::PoolId::from("slow_main")].a_to_b
            [strategy.binary_search_steps - 1]
            .amount_in;
        assert_eq!(*last_amount_in_a, strategy.slow_inventory.0);

        let last_amount_in_b = &precompute.pool_sims[&state::PoolId::from("slow_main")].b_to_a
            [strategy.binary_search_steps - 1]
            .amount_in;
        assert_eq!(*last_amount_in_b, strategy.slow_inventory.1);
    }

    #[test]
    fn precompute_different_decimals() {
        // Arrange
        let strategy = make_different_decimals_strategy();

        // Spot price: TEST->WETH = 0.001 (1m TEST : 1k WETH)
        let slow_state = make_slow_block_state_different_decimals(&strategy, 0, 1_000_000, 1_000);

        // Act
        let precompute = strategy
            .try_precompute(slow_state, None)
            .expect("precompute shouldn't fail");
        assert_eq!(precompute.block_height, 0);

        // Assert
        // correct spot prices
        assert_eq!(
            precompute.sorted_prices_a_b[0],
            (state::PoolId::from("slow_main"), "0.001".parse().unwrap())
        );

        // assert that only one pool is simulated
        assert_eq!(precompute.pool_sims.len(), 1);
        assert_eq!(
            precompute.pool_sims[&state::PoolId::from("slow_main")]
                .a_to_b
                .len(),
            strategy.binary_search_steps
        );
        assert_eq!(
            precompute.pool_sims[&state::PoolId::from("slow_main")]
                .b_to_a
                .len(),
            strategy.binary_search_steps
        );

        // check valid first and last step inputs
        let first_a_to_b = &precompute.pool_sims[&state::PoolId::from("slow_main")].a_to_b[0];
        assert_eq!(
            first_a_to_b.amount_in,
            BigUint::from_str("3125000000").unwrap()
        );

        let first_b_to_a = &precompute.pool_sims[&state::PoolId::from("slow_main")].b_to_a[0];
        assert_eq!(
            first_b_to_a.amount_in,
            BigUint::from_str("6250000000000000000").unwrap()
        );

        // check valid last step inputs
        let last_amount_in_a = &precompute.pool_sims[&state::PoolId::from("slow_main")].a_to_b
            [strategy.binary_search_steps - 1]
            .amount_in;
        assert_eq!(*last_amount_in_a, strategy.slow_inventory.0);

        let last_amount_in_b = &precompute.pool_sims[&state::PoolId::from("slow_main")].b_to_a
            [strategy.binary_search_steps - 1]
            .amount_in;
        assert_eq!(*last_amount_in_b, strategy.slow_inventory.1);
    }

    /// Test A->B->A arb with same decimal tokens (PEPE/WETH, both 18 decimals).
    ///
    /// - Reserves: (10K PEPE, 5K WETH) on slow, (10K PEPE, 2K WETH) on fast
    /// - PEPE->WETH price = WETH_reserve / PEPE_reserve
    ///   - Slow: 5,000 / 10,000 = 0.5
    ///   - Fast: 2,000 / 10,000 = 0.2
    /// - Arb: PEPE is worth MORE on slow (0.5 > 0.2), so sell PEPE on slow, buy back on fast
    /// - Direction: A->B slow (PEPE->WETH), B->A fast (WETH->PEPE)
    /// - Uses `a_to_b` precompute
    /// - simulate_swap uses WETH->PEPE
    #[test]
    fn generate_signal_same_decimals_aba() {
        let strategy = make_same_decimals_strategy();

        // Slow: 10K PEPE : 5K WETH (1 PEPE = 0.5 WETH)
        let slow_state = make_slow_block_state_same_decimals(&strategy, 2000, 10_000, 5_000);

        // Fast: 10K PEPE : 2K WETH (1 PEPE = 0.2 WETH)
        let fast_state = make_fast_block_same_decimals(&strategy, 100, 10_000, 2_000);

        let precompute = strategy
            .try_precompute(slow_state, None)
            .expect("precompute shouldn't fail");

        let fast_sorted_spot_prices =
            try_make_sorted_spot_prices(&fast_state.pair_state, &strategy.fast_pair)
                .expect("failed to make sorted spot prices");

        // Arb: PEPE worth more on slow (0.5 > 0.2 WETH)
        // - Slow: sell PEPE for WETH at 0.5
        // - Fast: sell WETH for PEPE at 0.2 (buy back cheaper)
        let signal = strategy
            .generate_signal(
                &precompute,
                fast_state.pair_state.clone(),
                fast_sorted_spot_prices,
                0u64, // TODO: non-zero base fee
            )
            .unwrap();

        assert_eq!(signal.slow_pool_id, state::PoolId::from("slow_main"));
        assert_eq!(signal.fast_pool_id, state::PoolId::from("fast_main"));

        // assert pepe->weth (slow) and weth->pepe (fast) legs
        assert_eq!(signal.slow_swap_sim.token_in, make_mainnet_pepe());
        assert_eq!(signal.slow_swap_sim.token_out, make_mainnet_weth());
        assert_eq!(signal.fast_swap_sim.token_in, make_base_weth());
        assert_eq!(signal.fast_swap_sim.token_out, make_base_pepe());

        let expected_slow_sim = precompute
            .pool_sims
            .get(&PoolId::from("slow_main"))
            .unwrap()
            .a_to_b
            .last()
            .unwrap();
        assert_eq!(signal.slow_swap_sim.amount_in, expected_slow_sim.amount_in);
        assert_eq!(
            signal.slow_swap_sim.amount_out,
            expected_slow_sim.amount_out
        );

        // assert fast amount in = slow amount out with slippage adjustment
        let expected_fast_amount_in =
            bps_discount(&expected_slow_sim.amount_out, strategy.max_slippage_bps);
        assert_eq!(signal.fast_swap_sim.amount_in, expected_fast_amount_in);

        // assert fast amount out is calculated from the right pool
        let expected_fast_sim = simulate_swap_for_pool_id(
            "fast_main",
            expected_fast_amount_in,
            &make_base_weth(),
            &make_base_pepe(),
            fast_state.pair_state,
        );
        assert_eq!(
            signal.fast_swap_sim.amount_out,
            expected_fast_sim.amount_out
        );

        assert_eq!(
            signal.expected_profit,
            ExpectedProfit::try_from_swaps(
                expected_slow_sim,
                &expected_fast_sim,
                &precompute.prices_a_usdc,
                &precompute.prices_b_usdc,
                &precompute.prices_eth_usdc,
                precompute.base_fee,
                0u64, // TODO: non-zero base fee
                strategy.max_slippage_bps,
                strategy.congestion_risk_discount_bps,
            )
            .unwrap()
        )
    }

    /// Test B->A->B arb with same decimal tokens (PEPE/WETH, both 18 decimals).
    ///
    /// - Reserves: (5K PEPE, 10K WETH) on slow, (2K PEPE, 10K WETH) on fast
    /// - PEPE->WETH price = WETH_reserve / PEPE_reserve
    ///   - Slow: 10,000 / 5,000 = 2.0
    ///   - Fast: 10,000 / 2,000 = 5.0
    /// - Arb: On slow 1 WETH = 0.5 PEPE, on fast 1 WETH = 0.2 PEPE. WETH buys more PEPE on slow.
    /// - Direction: B->A slow (WETH->PEPE), A->B fast (PEPE->WETH)
    /// - Uses `b_to_a` precompute
    /// - simulate_swap uses PEPE->WETH
    #[test]
    fn generate_signal_same_decimals_bab() {
        let strategy = make_same_decimals_strategy();

        // Slow: 5K PEPE : 10K WETH (1 WETH = 0.5 PEPE)
        let slow_state = make_slow_block_state_same_decimals(&strategy, 2000, 5_000, 10_000);

        // Fast: 2K PEPE : 10K WETH (1 WETH = 0.2 PEPE)
        let fast_state = make_fast_block_same_decimals(&strategy, 100, 2_000, 10_000);

        let precompute = strategy
            .try_precompute(slow_state, None)
            .expect("precompute shouldn't fail");

        let fast_sorted_spot_prices =
            try_make_sorted_spot_prices(&fast_state.pair_state, &strategy.fast_pair)
                .expect("failed to make sorted spot prices");

        // Arb: WETH buys more PEPE on slow (0.5 > 0.2 PEPE)
        // - Slow: sell WETH for PEPE at 0.5
        // - Fast: sell PEPE for WETH at 0.2 (buy back cheaper)
        let signal = strategy
            .generate_signal(
                &precompute,
                fast_state.pair_state.clone(),
                fast_sorted_spot_prices,
                0u64, // TODO: non-zero base fee
            )
            .unwrap();

        assert_eq!(signal.slow_pool_id, state::PoolId::from("slow_main"));
        assert_eq!(signal.fast_pool_id, state::PoolId::from("fast_main"));

        // assert weth->pepe (slow) and pepe->weth (fast) legs
        assert_eq!(signal.slow_swap_sim.token_in, make_mainnet_weth());
        assert_eq!(signal.slow_swap_sim.token_out, make_mainnet_pepe());
        assert_eq!(signal.fast_swap_sim.token_in, make_base_pepe());
        assert_eq!(signal.fast_swap_sim.token_out, make_base_weth());

        let expected_slow_sim = precompute
            .pool_sims
            .get(&PoolId::from("slow_main"))
            .expect("main pool not found")
            .b_to_a
            .last()
            .expect("b_to_a swap not found");
        assert_eq!(signal.slow_swap_sim.amount_in, expected_slow_sim.amount_in);
        assert_eq!(
            signal.slow_swap_sim.amount_out,
            expected_slow_sim.amount_out
        );

        // assert fast amount in = slow amount out with slippage adjustment
        let expected_fast_amount_in =
            bps_discount(&expected_slow_sim.amount_out, strategy.max_slippage_bps);
        assert_eq!(signal.fast_swap_sim.amount_in, expected_fast_amount_in);

        // assert fast amount out is calculated from the right pool
        let expected_fast_sim = simulate_swap_for_pool_id(
            "fast_main",
            expected_fast_amount_in,
            &make_base_pepe(),
            &make_base_weth(),
            fast_state.pair_state,
        );
        assert_eq!(
            signal.fast_swap_sim.amount_out,
            expected_fast_sim.amount_out
        );

        assert_eq!(
            signal.expected_profit,
            ExpectedProfit::try_from_swaps(
                expected_slow_sim,
                &expected_fast_sim,
                &precompute.prices_a_usdc,
                &precompute.prices_b_usdc,
                &precompute.prices_eth_usdc,
                precompute.base_fee,
                0u64, // TODO: non-zero base fee
                strategy.max_slippage_bps,
                strategy.congestion_risk_discount_bps,
            )
            .unwrap()
        )
    }

    /// Test A->B->A arb with different decimal tokens (TEST6=6 decimals, WETH=18 decimals).
    ///
    /// - Reserves: (10M TEST6, 5K WETH) on slow, (10M TEST6, 2K WETH) on fast
    /// - TEST6->WETH price = WETH_reserve / TEST6_reserve
    ///   - Slow: 5,000 / 10,000,000 = 0.0005
    ///   - Fast: 2,000 / 10,000,000 = 0.0002
    /// - Arb: TEST6 is worth MORE on slow (0.0005 > 0.0002), so sell TEST6 on slow, buy back on fast
    /// - Direction: A->B slow (TEST6->WETH), B->A fast (WETH->TEST6)
    /// - Uses `a_to_b` precompute
    /// - simulate_swap uses WETH->TEST6
    #[test]
    fn generate_signal_different_decimals_aba() {
        let strategy = make_different_decimals_strategy();

        // Slow: 10M TEST6 : 5K WETH (1 TEST6 = 0.0005 WETH)
        let slow_state =
            make_slow_block_state_different_decimals(&strategy, 2000, 10_000_000, 5_000);

        // Fast: 10M TEST6 : 2K WETH (1 TEST6 = 0.0002 WETH)
        let fast_state = make_fast_block_different_decimals(&strategy, 100, 10_000_000, 2_000);

        let precompute = strategy
            .try_precompute(slow_state, None)
            .expect("precompute shouldn't fail");

        let fast_sorted_spot_prices =
            try_make_sorted_spot_prices(&fast_state.pair_state, &strategy.fast_pair)
                .expect("failed to make sorted spot prices");

        // Arb: TEST6 is worth more on slow (0.0005 > 0.0002 WETH)
        // - Slow: sell TEST6 for WETH at 0.0005
        // - Fast: sell WETH for TEST6 at 0.0002 (buy back cheaper)
        let signal = strategy
            .generate_signal(
                &precompute,
                fast_state.pair_state.clone(),
                fast_sorted_spot_prices,
                0u64, // TODO: non-zero base fee
            )
            .unwrap();

        assert_eq!(signal.slow_pool_id, state::PoolId::from("slow_main"));
        assert_eq!(signal.fast_pool_id, state::PoolId::from("fast_main"));

        // assert test6->weth (slow) and weth->test6 (fast) legs
        assert_eq!(signal.slow_swap_sim.token_in, make_mainnet_test_6_token());
        assert_eq!(signal.slow_swap_sim.token_out, make_mainnet_weth());
        assert_eq!(signal.fast_swap_sim.token_in, make_base_weth());
        assert_eq!(signal.fast_swap_sim.token_out, make_base_test_6_token());

        let expected_slow_sim = precompute
            .pool_sims
            .get(&PoolId::from("slow_main"))
            .unwrap()
            .a_to_b
            .last()
            .unwrap();
        assert_eq!(signal.slow_swap_sim.amount_in, expected_slow_sim.amount_in);
        assert_eq!(
            signal.slow_swap_sim.amount_out,
            expected_slow_sim.amount_out
        );

        // assert fast amount in = slow amount out with slippage adjustment
        let expected_fast_amount_in =
            bps_discount(&expected_slow_sim.amount_out, strategy.max_slippage_bps);
        assert_eq!(signal.fast_swap_sim.amount_in, expected_fast_amount_in);

        // assert fast amount out is calculated from the right pool
        let expected_fast_sim = simulate_swap_for_pool_id(
            "fast_main",
            expected_fast_amount_in,
            &make_base_weth(),
            &make_base_test_6_token(),
            fast_state.pair_state,
        );
        assert_eq!(
            signal.fast_swap_sim.amount_out,
            expected_fast_sim.amount_out
        );

        assert_eq!(
            signal.expected_profit,
            ExpectedProfit::try_from_swaps(
                expected_slow_sim,
                &expected_fast_sim,
                &precompute.prices_a_usdc,
                &precompute.prices_b_usdc,
                &precompute.prices_eth_usdc,
                precompute.base_fee,
                0u64, // TODO: non-zero base fee
                strategy.max_slippage_bps,
                strategy.congestion_risk_discount_bps,
            )
            .unwrap()
        )
    }

    /// Test B->A->B arb with different decimal tokens (TEST6=6 decimals, WETH=18 decimals).
    ///
    /// - Reserves: (5K TEST6, 10K WETH) on slow, (2K TEST6, 10K WETH) on fast
    /// - TEST6->WETH price = WETH_reserve / TEST6_reserve
    ///   - Slow: 10,000 / 5,000 = 2.0
    ///   - Fast: 10,000 / 2,000 = 5.0
    /// - Arb: On slow 1 WETH = 0.5 TEST6, on fast 1 WETH = 0.2 TEST6. WETH buys more TEST6 on slow.
    /// - Direction: B->A slow (WETH->TEST6), A->B fast (TEST6->WETH)
    /// - Uses `b_to_a` precompute
    /// - simulate_swap uses TEST6->WETH
    #[test]
    fn generate_signal_different_decimals_bab() {
        let strategy = make_different_decimals_strategy();

        // Slow: 5K TEST6 : 10K WETH (1 WETH = 0.5 TEST6)
        let slow_state = make_slow_block_state_different_decimals(&strategy, 2000, 5_000, 10_000);

        // Fast: 2K TEST6 : 10K WETH (1 WETH = 0.2 TEST6)
        let fast_state = make_fast_block_different_decimals(&strategy, 100, 2_000, 10_000);

        let precompute = strategy
            .try_precompute(slow_state, None)
            .expect("precompute shouldn't fail");

        let fast_sorted_spot_prices =
            try_make_sorted_spot_prices(&fast_state.pair_state, &strategy.fast_pair)
                .expect("failed to make sorted spot prices");

        // Arb: WETH buys more TEST6 on slow (0.5 > 0.2 TEST6)
        // - Slow: sell WETH for TEST6 at 0.5
        // - Fast: sell TEST6 for WETH at 0.2 (buy back cheaper)
        let signal = strategy
            .generate_signal(
                &precompute,
                fast_state.pair_state.clone(),
                fast_sorted_spot_prices,
                0u64, // TODO: non-zero base fee
            )
            .unwrap();

        assert_eq!(signal.slow_pool_id, state::PoolId::from("slow_main"));
        assert_eq!(signal.fast_pool_id, state::PoolId::from("fast_main"));

        // assert weth->test6 (slow) and test6->weth (fast) legs
        assert_eq!(signal.slow_swap_sim.token_in, make_mainnet_weth());
        assert_eq!(signal.slow_swap_sim.token_out, make_mainnet_test_6_token());
        assert_eq!(signal.fast_swap_sim.token_in, make_base_test_6_token());
        assert_eq!(signal.fast_swap_sim.token_out, make_base_weth());

        let expected_slow_sim = precompute
            .pool_sims
            .get(&PoolId::from("slow_main"))
            .unwrap()
            .b_to_a
            .last()
            .unwrap();
        assert_eq!(signal.slow_swap_sim.amount_in, expected_slow_sim.amount_in);
        assert_eq!(
            signal.slow_swap_sim.amount_out,
            expected_slow_sim.amount_out
        );

        // assert fast amount in = slow amount out with slippage adjustment
        let expected_fast_amount_in =
            bps_discount(&expected_slow_sim.amount_out, strategy.max_slippage_bps);
        assert_eq!(signal.fast_swap_sim.amount_in, expected_fast_amount_in);

        // assert fast amount out is calculated from the right pool
        let expected_fast_sim = simulate_swap_for_pool_id(
            "fast_main",
            expected_fast_amount_in,
            &make_base_test_6_token(),
            &make_base_weth(),
            fast_state.pair_state,
        );
        assert_eq!(
            signal.fast_swap_sim.amount_out,
            expected_fast_sim.amount_out
        );

        assert_eq!(
            signal.expected_profit,
            ExpectedProfit::try_from_swaps(
                expected_slow_sim,
                &expected_fast_sim,
                &precompute.prices_a_usdc,
                &precompute.prices_b_usdc,
                &precompute.prices_eth_usdc,
                precompute.base_fee,
                0u64, // TODO: non-zero base fee
                strategy.max_slippage_bps,
                strategy.congestion_risk_discount_bps,
            )
            .unwrap()
        )
    }

    /// Test single binary search step (binary_search_steps = 1).
    ///
    /// When there's only one simulation, the binary search loop doesn't run (left == right).
    /// The single-element handling added in the fix should return that signal if profitable.
    ///
    /// - Uses same_decimals setup with binary_search_steps = 1
    /// - Reserves: (10K PEPE, 5K WETH) on slow, (10K PEPE, 2K WETH) on fast
    /// - Should find a valid signal using the single precomputed simulation
    #[test]
    fn generate_signal_single_binary_search_step() {
        let strategy = make_same_decimals_strategy_one_step();

        // Slow: 10K PEPE : 5K WETH (1 PEPE = 0.5 WETH)
        let slow_state = make_slow_block_state_same_decimals(&strategy, 2000, 10_000, 5_000);

        // Fast: 10K PEPE : 2K WETH (1 PEPE = 0.2 WETH)
        let fast_state = make_fast_block_same_decimals(&strategy, 100, 10_000, 2_000);

        let precompute = strategy
            .try_precompute(slow_state, None)
            .expect("precompute shouldn't fail");

        // Verify only 1 simulation was created
        let pool_sims = precompute
            .pool_sims
            .get(&state::PoolId::from("slow_main"))
            .expect("slow_main pool not found");
        assert_eq!(pool_sims.a_to_b.len(), 1, "expected 1 a_to_b simulation");
        assert_eq!(pool_sims.b_to_a.len(), 1, "expected 1 b_to_a simulation");

        let fast_sorted_spot_prices =
            try_make_sorted_spot_prices(&fast_state.pair_state, &strategy.fast_pair)
                .expect("failed to make sorted spot prices");

        // Should find a valid signal even with single step
        let signal = strategy
            .generate_signal(
                &precompute,
                fast_state.pair_state.clone(),
                fast_sorted_spot_prices,
                0u64, // TODO: non-zero base fee
            )
            .expect("should find signal with single binary search step");

        // Verify basic signal properties
        assert_eq!(signal.slow_pool_id, state::PoolId::from("slow_main"));
        assert_eq!(signal.fast_pool_id, state::PoolId::from("fast_main"));
        assert_eq!(signal.slow_swap_sim.token_in, make_mainnet_pepe());
        assert_eq!(signal.slow_swap_sim.token_out, make_mainnet_weth());
    }

    /// Test that equal prices on both chains returns an error (no arbitrage opportunity).
    ///
    /// - Reserves: (10K PEPE, 5K WETH) on slow, (10K PEPE, 5K WETH) on fast
    /// - PEPE->WETH price = WETH_reserve / PEPE_reserve
    ///   - Slow: 5,000 / 10,000 = 0.5
    ///   - Fast: 5,000 / 10,000 = 0.5
    /// - No arb: prices are equal, no crossed pools
    #[test]
    fn generate_signal_no_arb_equal_prices() {
        let strategy = make_same_decimals_strategy();

        // Slow: 10K PEPE : 5K WETH (1 PEPE = 0.5 WETH)
        let slow_state = make_slow_block_state_same_decimals(&strategy, 2000, 10_000, 5_000);

        // Fast: 10K PEPE : 5K WETH (1 PEPE = 0.5 WETH) - same price as slow
        let fast_state = make_fast_block_same_decimals(&strategy, 100, 10_000, 5_000);

        let precompute = strategy
            .try_precompute(slow_state, None)
            .expect("precompute shouldn't fail");

        let fast_sorted_spot_prices =
            try_make_sorted_spot_prices(&fast_state.pair_state, &strategy.fast_pair)
                .expect("failed to make sorted spot prices");

        // No arb opportunity - prices are equal, so no profitable signal
        let result = strategy.generate_signal(
            &precompute,
            fast_state.pair_state.clone(),
            fast_sorted_spot_prices,
            0u64, // TODO: non-zero base fee
        );

        assert!(result.is_err(), "expected error but got: {:#?}", result);
        let err_chain = format!("{:?}", result.unwrap_err());
        // Check direction context
        assert!(
            err_chain.contains("no optimal signal for B->A (slow) and A->B (fast)"),
            "expected direction context but got: {}",
            err_chain
        );
        // Check underlying reason
        assert!(
            err_chain.contains("all signals failed to create during search"),
            "expected underlying reason but got: {}",
            err_chain
        );
    }

    /// Test single element case fails with equal prices (no arb opportunity).
    ///
    /// Same setup as generate_signal_no_arb_equal_prices but with binary_search_steps = 1.
    /// This exercises the single-element path (left == right) instead of the binary search loop.
    ///
    /// - Reserves: (10K PEPE, 5K WETH) on slow, (10K PEPE, 5K WETH) on fast
    /// - Equal prices = no arb, signal creation fails
    #[test]
    fn generate_signal_single_element_no_arb() {
        let strategy = make_same_decimals_strategy_one_step();

        // Slow: 10K PEPE : 5K WETH (1 PEPE = 0.5 WETH)
        let slow_state = make_slow_block_state_same_decimals(&strategy, 2000, 10_000, 5_000);

        // Fast: 10K PEPE : 5K WETH (1 PEPE = 0.5 WETH) - same price as slow
        let fast_state = make_fast_block_same_decimals(&strategy, 100, 10_000, 5_000);

        let precompute = strategy
            .try_precompute(slow_state, None)
            .expect("precompute shouldn't fail");

        // Verify only 1 simulation was created
        let pool_sims = precompute
            .pool_sims
            .get(&state::PoolId::from("slow_main"))
            .expect("slow_main pool not found");
        assert_eq!(pool_sims.a_to_b.len(), 1, "expected 1 a_to_b simulation");
        assert_eq!(pool_sims.b_to_a.len(), 1, "expected 1 b_to_a simulation");

        let fast_sorted_spot_prices =
            try_make_sorted_spot_prices(&fast_state.pair_state, &strategy.fast_pair)
                .expect("failed to make sorted spot prices");

        let result = strategy.generate_signal(
            &precompute,
            fast_state.pair_state.clone(),
            fast_sorted_spot_prices,
            0u64, // TODO: non-zero base fee
        );

        assert!(result.is_err(), "expected error but got: {:?}", result);
        let err_chain = format!("{:?}", result.unwrap_err());
        // Check direction context
        assert!(
            err_chain.contains("no optimal signal for"),
            "expected direction context but got: {}",
            err_chain
        );
        // Check single element path was taken
        assert!(
            err_chain.contains("failed to create signal from single precompute"),
            "expected single precompute error but got: {}",
            err_chain
        );
    }
}
