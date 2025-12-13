//! Cross-chain single-hop arbitrage strategy implementation.
//!
//! Detects price differences between DEX pools on slow and fast chains for a token pair.
//! Uses precomputed swap simulations on the slow chain and real-time simulation on the fast
//! chain to identify profitable arbitrage opportunities via binary search optimization.

use std::{collections::HashMap, sync::Arc};

use color_eyre::eyre::{self, Context, eyre};
use num_bigint::BigUint;
use tracing::{debug, error, instrument, trace, warn};
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

#[derive(Debug, Clone)]
pub struct Precomputes {
    pub block_height: u64,
    pub prices_a_b: SpotPrices,
    pub sorted_prices_a_b: Vec<(PoolId, f64)>,
    pub pool_sims: HashMap<state::PoolId, simulation::PoolSteps>,
    pub pool_metadata: HashMap<state::PoolId, Arc<ProtocolComponent>>,
    pub prices_a_usdc: Option<SpotPrices>,
    pub prices_b_usdc: Option<SpotPrices>,
}

// Implementation of the arbitrage strategy
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrossChainSingleHop {
    // TODO: make a (chain, pair, inventory) tuple?
    pub slow_pair: Pair,
    pub slow_usdc: Token,
    pub slow_token_a_usdc: Option<Pair>,
    pub slow_token_b_usdc: Option<Pair>,
    pub slow_chain: Chain,
    pub fast_pair: Pair,
    pub fast_token_a_usdc: Option<Pair>,
    pub fast_token_b_usdc: Option<Pair>,
    pub fast_usdc: Token,
    pub fast_chain: Chain,
    pub slow_inventory: (BigUint, BigUint),
    pub fast_inventory: (BigUint, BigUint),
    pub binary_search_steps: usize,
    pub max_slippage_bps: u64,
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
        trace!(
            %prices_a_b,
            block.height = prices_a_b.block_height,
            chain.name = %self.slow_chain.name,
            "✅ Generated spot prices"
        );

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
            trace!(
                %prices_a_usdc,
                block.height = prices_a_b.block_height,
                chain.name = %self.slow_chain.name,
                "✅ Generated spot prices"
            );
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
            trace!(
                %prices_b_usdc,
                "✅ Generated spot prices"
            );
            Some(prices_b_usdc)
        } else {
            trace!(pair = %self.slow_pair, "Skipping spot price simulation for token B == USDC");
            None
        };

        Ok(Precomputes {
            block_height,
            prices_a_b,
            sorted_prices_a_b,
            pool_sims,
            pool_metadata: state.pair_state.metadata.clone(),
            prices_a_usdc,
            prices_b_usdc,
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
                    debug!(
                        %slow_direction,
                        %spread,
                        %slow_price,
                        %fast_price,
                        %slow_id,
                        %fast_id,
                        "found crossed pools"
                    );

                    (slow_id, fast_id, slow_direction)
                },
            )
        {
            match direction {
                Direction::AtoB => {
                    if let Some(signal) = self.find_optimal_signal(
                        &precompute.pool_sims[&slow_id].a_to_b,
                        precompute.pool_metadata[&slow_id].clone(),
                        &slow_id,
                        precompute.block_height,
                        &precompute.prices_a_usdc,
                        &precompute.prices_b_usdc,
                        fast_state.states[&fast_id].as_ref(),
                        fast_state.metadata[&fast_id].clone(),
                        &fast_id,
                        fast_state.block_height,
                        &self.fast_inventory.1,
                    ) {
                        trace!(
                            slow_sim = %signal.slow_swap_sim,
                            fast_sim = %signal.fast_swap_sim,
                            signal.surplus = ?signal.surplus,
                            signal.expected_profit = ?signal.expected_profit,
                            "found optimal swap for A->B (slow) and B->A (fast)"
                        );
                        Ok(signal)
                    } else {
                        Err(eyre!(
                            "no optimal signal found for A->B (slow) and B->A (fast)"
                        ))
                    }
                }
                Direction::BtoA => {
                    if let Some(signal) = self.find_optimal_signal(
                        &precompute.pool_sims[&slow_id].b_to_a,
                        precompute.pool_metadata[&slow_id].clone(),
                        &slow_id,
                        precompute.block_height,
                        &precompute.prices_a_usdc,
                        &precompute.prices_b_usdc,
                        fast_state.states[&fast_id].as_ref(),
                        fast_state.metadata[&fast_id].clone(),
                        &fast_id,
                        fast_state.block_height,
                        &self.fast_inventory.0,
                    ) {
                        trace!(
                            slow_sim = %signal.slow_swap_sim,
                            fast_sim = %signal.fast_swap_sim,
                            signal.surplus = ?signal.surplus,
                            signal.expected_profit = ?signal.expected_profit,
                            "found optimal swap for B->A (slow) and A->B (fast)"
                        );
                        Ok(signal)
                    } else {
                        Err(eyre!(
                            "no optimal signal found for B->A (slow) and A->B (fast)"
                        ))
                    }
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
    /// This assumes simulations behave "unimodally", i.e. they have a single peak, in terms of
    /// amount_in -> amount_out.
    ///
    /// At each step, the search compares the middle element, `mid`, to the one immediately after it,
    /// `next`.
    /// If `mid` < `next`, the search continues in the right half of the array.
    /// If `mid` > `next`, the search continues in the left half of the array.
    ///
    /// Each step uses a precomputed slow chain `Swap` and the fast chain's `ProtocolSim` to create
    /// the fast chain's `Swap`, and the a candidate `signals::CrossChainSingleHop`. The signals'
    /// expected profits are compared to find the optimal signal.
    ///
    /// TODO: add slow_inventory to logs?
    #[allow(clippy::too_many_arguments)]
    fn find_optimal_signal(
        &self,
        // TODO: have an abstraction around slow = (height, pool_id, sims) and fast = (height, pool_id, protocol_sim, inventory)
        slow_sims: &[Swap],
        slow_protocol_component: Arc<ProtocolComponent>,
        slow_pool_id: &PoolId,
        slow_height: u64,
        slow_prices_a_usdc: &Option<SpotPrices>,
        slow_prices_b_usdc: &Option<SpotPrices>,
        fast_state: &dyn ProtocolSim,
        fast_protocol_component: Arc<ProtocolComponent>,
        fast_pool_id: &PoolId,
        fast_height: u64,
        fast_inventory: &BigUint,
    ) -> Option<signals::CrossChainSingleHop> {
        let (mut left, mut right) = (0, slow_sims.len() - 1);

        let mut best_signal: Option<signals::CrossChainSingleHop> = None;

        while left < right {
            let mid = (right + left) / 2;

            // make sims for mid
            let mid_signal = match self.try_signal_from_precompute(
                slow_sims[mid].clone(),
                slow_protocol_component.clone(),
                slow_pool_id,
                slow_height,
                slow_prices_a_usdc,
                slow_prices_b_usdc,
                fast_state,
                fast_protocol_component.clone(),
                fast_pool_id,
                fast_height,
                fast_inventory,
            ) {
                Ok(signal) => signal,
                Err(err) => {
                    trace!(index = mid, err = %err, "failed to make mid signal, searching over smaller values");
                    right = mid - 1;
                    continue;
                }
            };

            trace!(
                index = mid,
                surplus = %mid_signal.surplus,
                expected_profit = %mid_signal.expected_profit,
                "Generated mid candidate signal"
            );

            // make sims for mid+1
            // TODO: usdc prices in here
            let next_signal = match self.try_signal_from_precompute(
                slow_sims[mid + 1].clone(),
                slow_protocol_component.clone(),
                slow_pool_id,
                slow_height,
                slow_prices_a_usdc,
                slow_prices_b_usdc,
                fast_state,
                fast_protocol_component.clone(),
                fast_pool_id,
                fast_height,
                fast_inventory,
            ) {
                Ok(signal) => signal,
                Err(err) => {
                    trace!(index = mid+1, err = %err, "failed to make mid+1 signal, searching over smaller values");
                    right = mid;
                    continue;
                }
            };
            trace!(
                index = mid+1,
                surplus = %next_signal.surplus,
                expected_profit.a = %next_signal.expected_profit.token_amounts.0,
                expected_profit.b = %next_signal.expected_profit.token_amounts.1,
                "Generated mid+1 candidate signal"
            );

            // compare the expected profits
            // TODO: move this out to a function that compares two signals?
            if mid_signal.expected_profit.token_amounts.0
                < next_signal.expected_profit.token_amounts.0
            {
                // next is higher -> check to the right (try a higher amount_in)
                trace!(index = mid, left = %left, right = %right, "mid+1 signal has higher expected profit, continuing search");
                best_signal = Some(next_signal);
                left = mid + 1;
            } else {
                // next is lower -> check to the left (try a lower amount_in)
                trace!(index = mid, left = %left, right = %right, "mid+1 signal has lower expected profit, continuing search");
                right = mid;
            }
        }

        trace!(index = %left, found_signal = %best_signal.is_some(), "search complete");

        best_signal
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
        slow_prices_a_usdc: &Option<SpotPrices>,
        slow_prices_b_usdc: &Option<SpotPrices>,
        fast_state: &dyn ProtocolSim,
        fast_protocol_component: Arc<ProtocolComponent>,
        fast_pool_id: &PoolId,
        fast_height: u64,
        fast_inventory: &BigUint,
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
            slow_prices_a_usdc,
            slow_prices_b_usdc,
            &self.fast_chain,
            &self.fast_pair,
            fast_protocol_component,
            fast_pool_id,
            fast_height,
            fast_sim.clone(),
            self.max_slippage_bps,
            self.congestion_risk_discount_bps,
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
        signals::{ExpectedProfit, Surplus},
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

    const PEPE_ADDRESS: &str = "0x0000000000000000000000000000000000000000";
    const TEST_ADDRESS: &str = "0x0000000000000000000000000000000000000000";
    const WETH_ADDRESS: &str = "0x0000000000000000000000000000000000000002";
    const BASE_USDC_ADDRESS: &str = "0x0000000000000000000000000000000000000001";
    const MAINNET_USDC_ADDRESS: &str = "0x0000000000000000000000000000000000000003";

    fn init_tracing() {
        TELEMETRY_INIT.get_or_init(|| {
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

    fn make_mainnet_test_6_token() -> Token {
        make_6_dec_token(tycho_common::models::Chain::Ethereum, "TEST", TEST_ADDRESS)
    }

    fn make_base_pepe() -> Token {
        make_18_dec_token(tycho_common::models::Chain::Base, "PEPE", PEPE_ADDRESS)
    }

    fn make_base_test_6_token() -> Token {
        make_6_dec_token(tycho_common::models::Chain::Base, "TEST", TEST_ADDRESS)
    }

    fn make_mainnet_weth() -> Token {
        make_18_dec_token(tycho_common::models::Chain::Ethereum, "WETH", WETH_ADDRESS)
    }

    fn make_mainnet_usdc() -> Token {
        make_6_dec_token(
            tycho_common::models::Chain::Ethereum,
            "USDC",
            MAINNET_USDC_ADDRESS,
        )
    }

    fn make_base_weth() -> Token {
        make_18_dec_token(tycho_common::models::Chain::Base, "WETH", WETH_ADDRESS)
    }

    fn make_base_usdc() -> Token {
        make_6_dec_token(tycho_common::models::Chain::Base, "USDC", BASE_USDC_ADDRESS)
    }

    fn scale_by_decimals(amount: &BigUint, decimals: u32) -> BigUint {
        amount * BigUint::from(10u64).pow(decimals)
    }

    fn make_univ2_protocol_sim(reserve_a: &BigUint, reserve_b: &BigUint) -> Arc<dyn ProtocolSim> {
        use std::str::FromStr;
        use tycho_simulation::evm::protocol::uniswap_v2::state::UniswapV2State;

        let reserve_a_u256 = alloy::primitives::U256::from_str(&reserve_a.to_string()).unwrap();
        let reserve_b_u256 = alloy::primitives::U256::from_str(&reserve_b.to_string()).unwrap();

        Arc::new(UniswapV2State::new(reserve_a_u256, reserve_b_u256))
    }

    fn make_single_univ2_pair_state(
        pair: &Pair,
        block_height: u64,
        pool_id: &str,
        reserve_a: u64,
        reserve_b: u64,
        chain: tycho_common::models::Chain,
    ) -> PairState {
        PairState {
            states: HashMap::from([(
                state::PoolId::from(pool_id),
                make_univ2_protocol_sim(
                    &scale_by_decimals(&BigUint::from(reserve_a), pair.token_a().decimals),
                    &scale_by_decimals(&BigUint::from(reserve_b), pair.token_b().decimals),
                ),
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
        let pool_state = state
            .states
            .get(&pool_id)
            .expect(&format!("pool state not found for {}", pool_id));
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

        state::block::BlockState {
            pair_state,
            token_a_usdc_state,
            token_a_balance: BigUint::from(0u64),
            token_b_usdc_state,
            token_b_balance: BigUint::from(0u64),
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

        state::block::BlockState {
            pair_state,
            token_a_usdc_state,
            token_a_balance: BigUint::from(0u64),
            token_b_usdc_state,
            token_b_balance: BigUint::from(0u64),
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

        state::block::BlockState {
            pair_state,
            token_a_usdc_state,
            token_a_balance: BigUint::from(0u64),
            token_b_usdc_state,
            token_b_balance: BigUint::from(0u64),
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

        state::block::BlockState {
            pair_state,
            token_a_usdc_state,
            token_a_balance: BigUint::from(0u64),
            token_b_usdc_state,
            token_b_balance: BigUint::from(0u64),
        }
    }

    /// Creates a strategy that uses the same number of decimals for all tokens.
    /// Token A is pepe 0x000
    /// Token B is weth 0x002
    /// USDC is 0x003
    fn make_same_decimals_strategy() -> Arc<strategy::CrossChainSingleHop> {
        init_tracing();

        // custom pepe addr 0x0..0
        // custom weth addr 0x0..2
        // so pair order is always (pepe, weth) for uniswap zero2one
        let slow_chain = Chain::eth_mainnet();
        let slow_pair = Pair::new(make_mainnet_pepe(), make_mainnet_weth());
        let slow_usdc = make_mainnet_usdc();
        let slow_token_a_usdc = Pair::new(make_mainnet_pepe(), slow_usdc.clone());
        let slow_token_b_usdc = Pair::new(make_mainnet_weth(), slow_usdc.clone());
        let available_inventory_slow = (
            scale_by_decimals(&BigUint::from(50u64), slow_pair.token_a().decimals),
            scale_by_decimals(&BigUint::from(100u64), slow_pair.token_b().decimals),
        );

        let fast_chain = Chain::base_mainnet();
        let fast_pair = Pair::new(make_base_pepe(), make_base_weth());
        let fast_usdc = make_base_usdc();
        let fast_token_a_usdc = Pair::new(make_base_pepe(), fast_usdc.clone());
        let fast_token_b_usdc = Pair::new(make_base_weth(), fast_usdc.clone());
        let available_inventory_fast = (
            scale_by_decimals(&BigUint::from(200u64), fast_pair.token_a().decimals),
            scale_by_decimals(&BigUint::from(150u64), fast_pair.token_b().decimals),
        );

        Arc::new(CrossChainSingleHop {
            slow_chain,
            slow_usdc,
            slow_pair,
            slow_inventory: available_inventory_slow,
            fast_chain,
            fast_usdc,
            fast_pair,
            fast_inventory: available_inventory_fast,
            max_slippage_bps: 25, // 0.25%
            congestion_risk_discount_bps: 25,
            // min_profit_threshold: 0.5, // 0.5%
            binary_search_steps: 16,
            slow_token_a_usdc: Some(slow_token_a_usdc),
            slow_token_b_usdc: Some(slow_token_b_usdc),
            fast_token_a_usdc: Some(fast_token_a_usdc),
            fast_token_b_usdc: Some(fast_token_b_usdc),
        })
    }

    /// Creates a strategy that uses different number of decimals for tokens.
    /// Token A is weth 0x002
    /// Token B is PEPE 0x099
    /// USDC is 0x003
    fn make_different_decimals_strategy() -> Arc<strategy::CrossChainSingleHop> {
        init_tracing();

        // custom weth addr 0x0..2
        // custom test addr 0x0..99
        // so pair order is always (weth, test) for uniswap zero2one
        let slow_chain = Chain::eth_mainnet();
        let slow_usdc = make_mainnet_usdc();
        let slow_token_b = make_mainnet_test_6_token();
        let slow_pair = Pair::new(slow_token_b.clone(), make_mainnet_weth());
        let slow_token_a_usdc = Pair::new(make_mainnet_weth(), slow_usdc.clone());
        let slow_token_b_usdc = Pair::new(slow_token_b, slow_usdc.clone());
        let available_inventory_slow = (
            scale_by_decimals(&BigUint::from(50_000u64), slow_pair.token_a().decimals),
            scale_by_decimals(&BigUint::from(100u64), slow_pair.token_b().decimals),
        );

        let fast_chain = Chain::base_mainnet();
        let fast_usdc = make_base_usdc();
        let fast_token_a = make_base_test_6_token();
        let fast_pair = Pair::new(fast_token_a.clone(), make_base_weth());
        let fast_token_a_usdc = Pair::new(fast_token_a, fast_usdc.clone());
        let fast_token_b_usdc = Pair::new(make_base_weth(), fast_usdc.clone());
        let available_inventory_fast = (
            scale_by_decimals(&BigUint::from(200_000u64), fast_pair.token_a().decimals),
            scale_by_decimals(&BigUint::from(500u64), fast_pair.token_b().decimals),
        );

        Arc::new(CrossChainSingleHop {
            slow_chain,
            slow_usdc,
            slow_pair,
            slow_inventory: available_inventory_slow,
            fast_chain,
            fast_usdc,
            fast_pair,
            fast_inventory: available_inventory_fast,
            max_slippage_bps: 25, // 0.25%
            congestion_risk_discount_bps: 25,
            // min_profit_threshold: 0.5, // 0.5%
            binary_search_steps: 16,
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

    #[test]
    fn generate_signal_same_decimals_aba() {
        let strategy = make_same_decimals_strategy();

        // Slow: PEPE->WETH = 0.5 (10k PEPE : 5k WETH) - expensive WETH on slow chain
        let slow_state = make_slow_block_state_same_decimals(&strategy, 2000, 10_000, 5_000);

        // Fast: PEPE->WETH = 0.2 (10k PEPE : 2k WETH) - cheap WETH on fast chain
        // Arb: Sell PEPE on slow for WETH, buy PEPE on fast with WETH
        let fast_state = make_fast_block_same_decimals(&strategy, 100, 10_000, 2_000);

        let precompute = strategy
            .try_precompute(slow_state, None)
            .expect("precompute shouldn't fail");

        let fast_sorted_spot_prices =
            try_make_sorted_spot_prices(&fast_state.pair_state, &strategy.fast_pair)
                .expect("failed to make sorted spot prices");

        let signal = strategy
            .generate_signal(
                &precompute,
                fast_state.pair_state.clone(),
                fast_sorted_spot_prices,
            )
            .unwrap();

        assert_eq!(signal.slow_pool_id, state::PoolId::from("slow_main"));
        assert_eq!(signal.fast_pool_id, state::PoolId::from("fast_main"));

        // assert pepe->weth and weth->pepe legs
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
            signal.surplus,
            Surplus::try_from_swaps(
                &expected_slow_sim,
                &expected_fast_sim,
                &precompute.prices_a_usdc,
                &precompute.prices_b_usdc,
                strategy.max_slippage_bps
            )
            .unwrap()
        );
        assert_eq!(
            signal.expected_profit,
            ExpectedProfit::try_from_swaps(
                &expected_slow_sim,
                &expected_fast_sim,
                &precompute.prices_a_usdc,
                &precompute.prices_b_usdc,
                strategy.max_slippage_bps,
                strategy.congestion_risk_discount_bps,
            )
            .unwrap()
        )
    }

    #[test]
    fn generate_signal_same_decimals_bab() {
        let strategy = make_same_decimals_strategy();

        let slow_state = make_slow_block_state_same_decimals(&strategy, 2000, 5_000, 10_000);

        let fast_state = make_fast_block_same_decimals(&strategy, 100, 2_000, 10_000);

        let precompute = strategy
            .try_precompute(slow_state, None)
            .expect("precompute shouldn't fail");

        let fast_sorted_spot_prices =
            try_make_sorted_spot_prices(&fast_state.pair_state, &strategy.fast_pair)
                .expect("failed to make sorted spot prices");

        let signal = strategy
            .generate_signal(
                &precompute,
                fast_state.pair_state.clone(),
                fast_sorted_spot_prices,
            )
            .unwrap();

        assert_eq!(signal.slow_pool_id, state::PoolId::from("slow_main"));
        assert_eq!(signal.fast_pool_id, state::PoolId::from("fast_main"));

        // assert pepe->weth and weth->pepe legs
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
            signal.surplus,
            Surplus::try_from_swaps(
                &expected_slow_sim,
                &expected_fast_sim,
                &precompute.prices_a_usdc,
                &precompute.prices_b_usdc,
                strategy.max_slippage_bps
            )
            .unwrap()
        );
        assert_eq!(
            signal.expected_profit,
            ExpectedProfit::try_from_swaps(
                &expected_slow_sim,
                &expected_fast_sim,
                &precompute.prices_a_usdc,
                &precompute.prices_b_usdc,
                strategy.max_slippage_bps,
                strategy.congestion_risk_discount_bps,
            )
            .unwrap()
        )
    }
    #[test]
    fn generate_signal_different_decimals_aba() {
        let strategy = make_different_decimals_strategy();

        // WETH -> TEST price is 200
        let slow_state =
            make_slow_block_state_different_decimals(&strategy, 2000, 10_000_000, 5_000);

        // WETH -> TEST price is 500
        let fast_state = make_fast_block_different_decimals(&strategy, 100, 10_000_000, 2_000);

        let precompute = strategy
            .try_precompute(slow_state, None)
            .expect("precompute shouldn't fail");

        let fast_sorted_spot_prices =
            try_make_sorted_spot_prices(&fast_state.pair_state, &strategy.fast_pair)
                .expect("failed to make sorted spot prices");

        let signal = strategy
            .generate_signal(
                &precompute,
                fast_state.pair_state.clone(),
                fast_sorted_spot_prices,
            )
            .unwrap();

        assert_eq!(signal.slow_pool_id, state::PoolId::from("slow_main"));
        assert_eq!(signal.fast_pool_id, state::PoolId::from("fast_main"));

        // assert test->weth and test->pepe legs
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
            &make_base_usdc(),
            fast_state.pair_state,
        );
        assert_eq!(
            signal.fast_swap_sim.amount_out,
            expected_fast_sim.amount_out
        );

        assert_eq!(
            signal.surplus,
            Surplus::try_from_swaps(
                &expected_slow_sim,
                &expected_fast_sim,
                &precompute.prices_a_usdc,
                &precompute.prices_b_usdc,
                strategy.max_slippage_bps
            )
            .unwrap()
        );
        assert_eq!(
            signal.expected_profit,
            ExpectedProfit::try_from_swaps(
                &expected_slow_sim,
                &expected_fast_sim,
                &precompute.prices_a_usdc,
                &precompute.prices_b_usdc,
                strategy.max_slippage_bps,
                strategy.congestion_risk_discount_bps,
            )
            .unwrap()
        )
    }

    #[test]
    fn generate_signal_different_decimals_bab() {
        let strategy = make_different_decimals_strategy();

        // weth -> PEPE price is 0.5
        let slow_state = make_slow_block_state_different_decimals(&strategy, 2000, 5_000, 10_000);

        // weth -> PEPE price is 0.2
        let fast_state = make_fast_block_different_decimals(&strategy, 100, 2_000, 10_000);

        let precompute = strategy
            .try_precompute(slow_state, None)
            .expect("precompute shouldn't fail");

        let fast_sorted_spot_prices =
            try_make_sorted_spot_prices(&fast_state.pair_state, &strategy.fast_pair)
                .expect("failed to make sorted spot prices");

        let signal = strategy
            .generate_signal(
                &precompute,
                fast_state.pair_state.clone(),
                fast_sorted_spot_prices,
            )
            .unwrap();

        assert_eq!(signal.slow_pool_id, state::PoolId::from("slow_main"));
        assert_eq!(signal.fast_pool_id, state::PoolId::from("fast_main"));

        // assert pepe->weth and weth->pepe legs
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
            &make_base_pepe(),
            &make_base_weth(),
            fast_state.pair_state,
        );
        assert_eq!(
            signal.fast_swap_sim.amount_out,
            expected_fast_sim.amount_out
        );

        assert_eq!(
            signal.surplus,
            Surplus::try_from_swaps(
                &expected_slow_sim,
                &expected_fast_sim,
                &precompute.prices_a_usdc,
                &precompute.prices_b_usdc,
                strategy.max_slippage_bps
            )
            .unwrap()
        );
        assert_eq!(
            signal.expected_profit,
            ExpectedProfit::try_from_swaps(
                &expected_slow_sim,
                &expected_fast_sim,
                &precompute.prices_a_usdc,
                &precompute.prices_b_usdc,
                strategy.max_slippage_bps,
                strategy.congestion_risk_discount_bps,
            )
            .unwrap()
        )
    }
}
