//! Cross-chain single-hop arbitrage strategy implementation.
//!
//! Detects price differences between DEX pools on slow and fast chains for a token pair.
//! Uses precomputed swap simulations on the slow chain and real-time simulation on the fast
//! chain to identify profitable arbitrage opportunities via binary search optimization.

use std::sync::Arc;

use color_eyre::eyre::{self, Context, eyre};
use num_bigint::BigUint;
use tracing::{debug, instrument, trace};
use tycho_common::models::token::Token;
use tycho_simulation::{
    protocol::models::ProtocolComponent, tycho_core::simulation::protocol_sim::ProtocolSim,
};

use crate::{
    chain::Chain,
    signals::{self, Direction, bps_discount},
    state::{
        self, PoolId,
        block::BlockState,
        pair::{Pair, PairState},
    },
};

mod builder;
mod precompute;
pub mod simulation;
pub use builder::Builder;
pub use precompute::Precomputes;
pub use simulation::Swap;

// Implementation of the arbitrage strategy
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrossChainSingleHop {
    // TODO: make a (chain, pair, inventory) tuple?
    pub slow_pair: Pair,
    pub slow_usdc: Token,
    pub slow_chain: Chain,
    pub fast_pair: Pair,
    pub fast_usdc: Token,
    pub fast_chain: Chain,
    pub slow_inventory: (BigUint, BigUint),
    pub fast_inventory: (BigUint, BigUint),
    pub binary_search_steps: usize,
    pub max_slippage_bps: u64,
    pub congestion_risk_discount_bps: u64,
}

impl CrossChainSingleHop {
    #[instrument(skip_all)]
    pub fn precompute(&self, slow_state: BlockState) -> Precomputes {
        Precomputes::from_pair_state(
            &slow_state.pair_state,
            &self.slow_pair,
            &self.slow_inventory,
            None,
            self.binary_search_steps,
        )
    }

    #[instrument(skip_all, fields(
        slow.chain = %self.slow_chain,
        slow.pair = %self.slow_pair,
        slow.height = %precompute.block_height,
        fast.chain = %self.fast_chain,
        fast.pair = %self.fast_pair,
        fast.height = %fast_state.block_height
    ))]
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
            find_first_crossed_pools(&precompute.sorted_spot_prices, &fast_sorted_spot_prices).map(
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
            // TODO: feed token_a_usdc_price and token_b_usdc_price here as well
            match direction {
                Direction::AtoB => {
                    if let Some(signal) = self.find_optimal_signal(
                        &precompute.pool_sims[&slow_id].a_to_b,
                        precompute.pool_metadata[&slow_id].clone(),
                        &slow_id,
                        precompute.block_height,
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
                        fast_state.states[&fast_id].as_ref(),
                        fast_state.metadata[&fast_id].clone(),
                        &fast_id,
                        fast_state.block_height,
                        &self.fast_inventory.0,
                    ) {
                        trace!(slow_sim = %signal.slow_swap_sim, fast_sim = %signal.fast_swap_sim, signal.surplus = ?signal.surplus, signal.expected_profit = ?signal.expected_profit, "found optimal swap for B->A (slow) and A->B (fast)");
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
                surplus.a = %mid_signal.surplus.0,
                surplus.b = %mid_signal.surplus.1,
                expected_profit.a = %mid_signal.expected_profit.0,
                expected_profit.b = %mid_signal.expected_profit.1,
                "Generated mid candidate signal"
            );

            // make sims for mid+1
            // TODO: usdc prices in here
            let next_signal = match self.try_signal_from_precompute(
                slow_sims[mid + 1].clone(),
                slow_protocol_component.clone(),
                slow_pool_id,
                slow_height,
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
                surplus.a = %next_signal.surplus.0,
                surplus.b = %next_signal.surplus.1,
                expected_profit.a = %next_signal.expected_profit.0,
                expected_profit.b = %next_signal.expected_profit.1,
                "Generated mid+1 candidate signal"
            );

            // compare the expected profits
            // TODO: is this the correct value to compare?
            // TODO: move this out to a function that compares two signals?
            if mid_signal.expected_profit < next_signal.expected_profit {
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

        // TODO: usdc prices
        signals::CrossChainSingleHop::try_from_simulations(
            &self.slow_chain,
            &self.slow_pair,
            slow_protocol_component,
            slow_pool_id,
            slow_height,
            slow_sim.clone(),
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
            trace!(%slow_sim, %fast_sim, %err,
                    "‼️ failed to make signal");
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
        signals::{calculate_expected_profits, calculate_surplus},
        state::{self, pair::PairState},
        strategy::{self, CrossChainSingleHop, simulation::make_sorted_spot_prices},
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

    fn make_18_dec_token(chain: tycho_common::models::Chain, symbol: &str) -> Token {
        Token::new(
            // 0x0..00 address for uniswap zero2one pool order
            &tycho_common::Bytes::from_str("0x0000000000000000000000000000000000000000").unwrap(),
            symbol,
            18,
            1000,
            &[Some(1000u64)],
            chain,
            100,
        )
    }

    #[allow(dead_code)]
    fn make_6_dec_token(chain: tycho_common::models::Chain, symbol: &str) -> Token {
        Token::new(
            // 0x0..03 address for uniswap zero2one pool order
            &tycho_common::Bytes::from_str("0x0000000000000000000000000000000000000003").unwrap(),
            symbol,
            6,
            1000,
            &[Some(1000u64)],
            chain,
            100,
        )
    }

    fn make_mainnet_pepe() -> Token {
        make_18_dec_token(tycho_common::models::Chain::Ethereum, "PEPE")
    }

    fn make_base_pepe() -> Token {
        make_18_dec_token(tycho_common::models::Chain::Base, "PEPE")
    }

    fn make_mainnet_weth() -> Token {
        Token::new(
            // 0x0..02 address for uniswap zero2one pool order
            &tycho_common::Bytes::from_str("0x0000000000000000000000000000000000000002").unwrap(),
            "WETH",
            18,
            1000,
            &[Some(1000u64)],
            tycho_common::models::Chain::Ethereum,
            100,
        )
    }

    fn make_mainnet_usdc() -> Token {
        Token::new(
            // 0x0..01 address for uniswap zero2one pool order
            &tycho_common::Bytes::from_str("0x0000000000000000000000000000000000000001").unwrap(),
            "USDC",
            6,
            1000,
            &[Some(1000u64)],
            tycho_common::models::Chain::Ethereum,
            100,
        )
    }

    fn make_base_weth() -> Token {
        Token::new(
            // 0x0..02 address for uniswap zero2one pool order
            &tycho_common::Bytes::from_str("0x0000000000000000000000000000000000000002").unwrap(),
            "WETH",
            18,
            1000,
            &[Some(1000u64)],
            tycho_common::models::Chain::Base,
            100,
        )
    }

    fn make_base_usdc() -> Token {
        Token::new(
            // 0x0..01 address for uniswap zero2one pool order
            &tycho_common::Bytes::from_str("0x0000000000000000000000000000000000000001").unwrap(),
            "USDC",
            6,
            1000,
            &[Some(1000u64)],
            tycho_common::models::Chain::Base,
            100,
        )
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

    fn make_mainnet_weth_usdc_pair() -> Pair {
        let weth = make_mainnet_weth();
        let usdc = make_mainnet_usdc();
        Pair::new(weth, usdc)
    }

    fn make_base_weth_usdc_pair() -> Pair {
        let weth = make_base_weth();
        let usdc = make_base_usdc();
        Pair::new(weth, usdc)
    }

    fn make_mainnet_pepe_usdc_pair() -> Pair {
        let pepe = make_mainnet_pepe();
        let usdc = make_mainnet_usdc();
        Pair::new(pepe, usdc)
    }

    fn make_base_pepe_usdc_pair() -> Pair {
        let pepe = make_base_pepe();
        let usdc = make_base_usdc();
        Pair::new(pepe, usdc)
    }

    fn make_mainnet_weth_usdc_univ2_pair_state() -> PairState {
        let pair = make_mainnet_weth_usdc_pair();
        make_single_univ2_pair_state(
            &pair,
            0,
            "0x0000000000000000000000000000000000000011",
            500000000000000000,
            1000000000000000000,
            tycho_common::models::Chain::Ethereum,
        )
    }

    fn make_mainnet_pepe_usdc_univ2_pair_state() -> PairState {
        let pair = make_mainnet_pepe_usdc_pair();
        make_single_univ2_pair_state(
            &pair,
            0,
            "0x0000000000000000000000000000000000000022",
            1000000000000000000,
            500000000000000000,
            tycho_common::models::Chain::Ethereum,
        )
    }

    fn make_base_weth_usdc_univ2_pair_state() -> PairState {
        let pair = make_base_weth_usdc_pair();
        make_single_univ2_pair_state(
            &pair,
            0,
            "0x0000000000000000000000000000000000000011",
            500000000000000000,
            1000000000000000000,
            tycho_common::models::Chain::Base,
        )
    }

    fn make_base_pepe_usdc_univ2_pair_state() -> PairState {
        let pair = make_base_pepe_usdc_pair();
        make_single_univ2_pair_state(
            &pair,
            0,
            "0x0000000000000000000000000000000000000022",
            1000000000000000000,
            500000000000000000,
            tycho_common::models::Chain::Base,
        )
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
        let pool_state = state.states.get(&pool_id).unwrap();
        Swap::from_protocol_sim(&amount_in, token_in, token_out, pool_state.as_ref()).unwrap()
    }

    fn make_same_decimals_strategy() -> Arc<strategy::CrossChainSingleHop> {
        init_tracing();

        // custom pepe addr 0x0..0
        // custom weth addr 0x0..2
        // so pair order is always (pepe, weth) for uniswap zero2one
        let slow_chain = Chain::eth_mainnet();
        let slow_pair = Pair::new(make_mainnet_pepe(), make_mainnet_weth());
        let slow_usdc = make_mainnet_usdc();
        let available_inventory_slow = (
            scale_by_decimals(&BigUint::from(50u64), slow_pair.token_a().decimals),
            scale_by_decimals(&BigUint::from(100u64), slow_pair.token_b().decimals),
        );

        let fast_chain = Chain::base_mainnet();
        let fast_usdc = make_base_usdc();
        let fast_pair = Pair::new(make_base_pepe(), make_base_weth());
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
        })
    }

    fn make_different_decimals_strategy() -> Arc<strategy::CrossChainSingleHop> {
        init_tracing();

        // custom usdc addr 0x0..1
        // custom weth addr 0x0..2
        // so pair order is always (usdc, weth) for uniswap zero2one
        let slow_chain = Chain::eth_mainnet();
        let slow_usdc = make_mainnet_usdc();
        let slow_pair = Pair::new(make_mainnet_usdc(), make_mainnet_weth());
        let available_inventory_slow = (
            scale_by_decimals(&BigUint::from(50_000u64), slow_pair.token_a().decimals),
            scale_by_decimals(&BigUint::from(100u64), slow_pair.token_b().decimals),
        );

        let fast_chain = Chain::base_mainnet();
        let fast_usdc = make_base_usdc();
        let fast_pair = Pair::new(make_base_usdc(), make_base_weth());
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
        })
    }

    #[test]
    fn precompute_same_decimals() {
        // Arrange
        // slow chain inventory is 100,000 PEPE and 50 ETH
        let strategy = make_same_decimals_strategy();

        // 0x123 -> univ2(1m, 1k)
        // spot price should be ~1000/ or 0.001
        let slow_state = make_single_univ2_pair_state(
            &strategy.slow_pair,
            0,
            "0x123",
            1_000_000,
            1_000,
            tycho_common::models::Chain::Ethereum,
        );

        // Act
        let precompute = strategy.precompute(slow_state.clone());
        assert_eq!(precompute.block_height, 0);

        // Assert
        // correct spot prices
        assert_eq!(
            precompute.sorted_spot_prices[0],
            (state::PoolId::from("0x123"), "0.001".parse().unwrap())
        );

        // assert that only one pool is simulated
        assert_eq!(precompute.pool_sims.len(), 1);
        assert_eq!(
            precompute.pool_sims[&state::PoolId::from("0x123")]
                .a_to_b
                .len(),
            strategy.binary_search_steps
        );
        assert_eq!(
            precompute.pool_sims[&state::PoolId::from("0x123")]
                .b_to_a
                .len(),
            strategy.binary_search_steps
        );

        // check valid first and last step inputs
        // 100,000 PEPE inventory / 5 steps  = 20,000 PEPE
        let first_a_to_b = &precompute.pool_sims[&state::PoolId::from("0x123")].a_to_b[0];
        assert_eq!(
            first_a_to_b.amount_in,
            BigUint::from_str("3125000000000000000").unwrap()
        );
        // 50 ETH / 5 steps = 10 ETH
        let first_b_to_a = &precompute.pool_sims[&state::PoolId::from("0x123")].b_to_a[0];
        assert_eq!(
            first_b_to_a.amount_in,
            BigUint::from_str("6250000000000000000").unwrap()
        );

        // check valid last step inputs
        // 100,000 PEPE
        let last_amount_in_a = &precompute.pool_sims[&state::PoolId::from("0x123")].a_to_b
            [strategy.binary_search_steps - 1]
            .amount_in;
        assert_eq!(*last_amount_in_a, strategy.slow_inventory.0);

        // 50 ETH
        let last_amount_in_b = &precompute.pool_sims[&state::PoolId::from("0x123")].b_to_a
            [strategy.binary_search_steps - 1]
            .amount_in;
        assert_eq!(*last_amount_in_b, strategy.slow_inventory.1);
    }

    #[test]
    fn precompute_different_decimals() {
        // Arrange
        // slow chain inventory is 100,000 PEPE and 50 ETH
        let strategy = make_different_decimals_strategy();

        // 0x123 -> univ2(1m, 1k)
        // spot price should be ~1000/ or 0.001
        let slow_state = make_single_univ2_pair_state(
            &strategy.slow_pair,
            0,
            "0x123",
            1_000_000,
            1_000,
            tycho_common::models::Chain::Ethereum,
        );

        // Act
        let precompute = strategy.precompute(slow_state.clone());
        assert_eq!(precompute.block_height, 0);

        // Assert
        // correct spot prices
        assert_eq!(
            precompute.sorted_spot_prices[0],
            (state::PoolId::from("0x123"), "0.001".parse().unwrap())
        );

        // assert that only one pool is simulated
        assert_eq!(precompute.pool_sims.len(), 1);
        assert_eq!(
            precompute.pool_sims[&state::PoolId::from("0x123")]
                .a_to_b
                .len(),
            strategy.binary_search_steps
        );
        assert_eq!(
            precompute.pool_sims[&state::PoolId::from("0x123")]
                .b_to_a
                .len(),
            strategy.binary_search_steps
        );

        // check valid first and last step inputs
        // 100,000 PEPE inventory / 5 steps  = 20,000 PEPE
        let first_a_to_b = &precompute.pool_sims[&state::PoolId::from("0x123")].a_to_b[0];
        assert_eq!(
            first_a_to_b.amount_in,
            BigUint::from_str("3125000000").unwrap()
        );
        // 50 ETH / 5 steps = 10 ETH
        let first_b_to_a = &precompute.pool_sims[&state::PoolId::from("0x123")].b_to_a[0];
        assert_eq!(
            first_b_to_a.amount_in,
            BigUint::from_str("6250000000000000000").unwrap()
        );

        // check valid last step inputs
        // 100,000 PEPE
        let last_amount_in_a = &precompute.pool_sims[&state::PoolId::from("0x123")].a_to_b
            [strategy.binary_search_steps - 1]
            .amount_in;
        assert_eq!(*last_amount_in_a, strategy.slow_inventory.0);

        // 50 ETH
        let last_amount_in_b = &precompute.pool_sims[&state::PoolId::from("0x123")].b_to_a
            [strategy.binary_search_steps - 1]
            .amount_in;
        assert_eq!(*last_amount_in_b, strategy.slow_inventory.1);
    }

    #[test]
    fn generate_signal_same_decimals_aba() {
        let strategy = make_same_decimals_strategy();

        let slow_state = make_single_univ2_pair_state(
            &strategy.slow_pair,
            2000,
            "0x123",
            10_000,
            5_000,
            tycho_common::models::Chain::Ethereum,
        );

        let fast_state = make_single_univ2_pair_state(
            &strategy.fast_pair,
            100,
            "0x456",
            10_000,
            2_000,
            tycho_common::models::Chain::Base,
        );

        let precompute = strategy.precompute(slow_state);
        let fast_sorted_spot_prices = make_sorted_spot_prices(&fast_state, &strategy.fast_pair);
        let signal = strategy
            .generate_signal(&precompute, fast_state.clone(), fast_sorted_spot_prices)
            .unwrap();

        assert_eq!(signal.slow_pool_id, state::PoolId::from("0x123"));
        assert_eq!(signal.fast_pool_id, state::PoolId::from("0x456"));

        // assert pepe->weth and weth->pepe legs
        assert_eq!(signal.slow_swap_sim.token_in, make_mainnet_pepe());
        assert_eq!(signal.slow_swap_sim.token_out, make_mainnet_weth());
        assert_eq!(signal.fast_swap_sim.token_in, make_base_weth());
        assert_eq!(signal.fast_swap_sim.token_out, make_base_pepe());

        let expected_slow_sim = precompute
            .pool_sims
            .get(&PoolId::from("0x123"))
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
            "0x456",
            expected_fast_amount_in,
            &make_base_weth(),
            &make_base_pepe(),
            fast_state,
        );
        assert_eq!(
            signal.fast_swap_sim.amount_out,
            expected_fast_sim.amount_out
        );

        assert_eq!(
            signal.surplus,
            calculate_surplus(&expected_slow_sim, &expected_fast_sim).unwrap()
        );
        assert_eq!(
            signal.expected_profit,
            calculate_expected_profits(
                &expected_slow_sim,
                &expected_fast_sim,
                strategy.max_slippage_bps,
                strategy.congestion_risk_discount_bps
            )
            .unwrap()
        )
    }

    #[test]
    fn generate_signal_same_decimals_bab() {
        let strategy = make_same_decimals_strategy();

        let slow_state = make_single_univ2_pair_state(
            &strategy.slow_pair,
            2000,
            "0x123",
            5_000,
            10_000,
            tycho_common::models::Chain::Ethereum,
        );

        let fast_state = make_single_univ2_pair_state(
            &strategy.fast_pair,
            100,
            "0x456",
            2_000,
            10_000,
            tycho_common::models::Chain::Ethereum,
        );

        let precompute = strategy.precompute(slow_state);
        let fast_sorted_spot_prices = make_sorted_spot_prices(&fast_state, &strategy.fast_pair);
        let signal = strategy
            .generate_signal(&precompute, fast_state.clone(), fast_sorted_spot_prices)
            .unwrap();

        assert_eq!(signal.slow_pool_id, state::PoolId::from("0x123"));
        assert_eq!(signal.fast_pool_id, state::PoolId::from("0x456"));

        // assert pepe->weth and weth->pepe legs
        assert_eq!(signal.slow_swap_sim.token_in, make_mainnet_weth());
        assert_eq!(signal.slow_swap_sim.token_out, make_mainnet_pepe());
        assert_eq!(signal.fast_swap_sim.token_in, make_base_pepe());
        assert_eq!(signal.fast_swap_sim.token_out, make_base_weth());

        let expected_slow_sim = precompute
            .pool_sims
            .get(&PoolId::from("0x123"))
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
            "0x456",
            expected_fast_amount_in,
            &make_base_pepe(),
            &make_base_weth(),
            fast_state,
        );
        assert_eq!(
            signal.fast_swap_sim.amount_out,
            expected_fast_sim.amount_out
        );

        assert_eq!(
            signal.surplus,
            calculate_surplus(&expected_slow_sim, &expected_fast_sim).unwrap()
        );
        assert_eq!(
            signal.expected_profit,
            calculate_expected_profits(
                &expected_slow_sim,
                &expected_fast_sim,
                strategy.max_slippage_bps,
                strategy.congestion_risk_discount_bps
            )
            .unwrap()
        )
    }
    #[test]
    fn generate_signal_different_decimals_aba() {
        let strategy = make_different_decimals_strategy();

        let slow_state = make_single_univ2_pair_state(
            &strategy.slow_pair,
            2000,
            "0x123",
            10_000_000,
            5_000,
            tycho_common::models::Chain::Ethereum,
        );

        let fast_state = make_single_univ2_pair_state(
            &strategy.fast_pair,
            100,
            "0x456",
            10_000_000,
            2_000,
            tycho_common::models::Chain::Base,
        );

        let precompute = strategy.precompute(slow_state);
        let fast_sorted_spot_prices = make_sorted_spot_prices(&fast_state, &strategy.fast_pair);
        let signal = strategy
            .generate_signal(&precompute, fast_state.clone(), fast_sorted_spot_prices)
            .unwrap();

        assert_eq!(signal.slow_pool_id, state::PoolId::from("0x123"));
        assert_eq!(signal.fast_pool_id, state::PoolId::from("0x456"));

        // assert pepe->weth and weth->pepe legs
        assert_eq!(signal.slow_swap_sim.token_in, make_mainnet_usdc());
        assert_eq!(signal.slow_swap_sim.token_out, make_mainnet_weth());
        assert_eq!(signal.fast_swap_sim.token_in, make_base_weth());
        assert_eq!(signal.fast_swap_sim.token_out, make_base_usdc());

        let expected_slow_sim = precompute
            .pool_sims
            .get(&PoolId::from("0x123"))
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
            "0x456",
            expected_fast_amount_in,
            &make_base_weth(),
            &make_base_usdc(),
            fast_state,
        );
        assert_eq!(
            signal.fast_swap_sim.amount_out,
            expected_fast_sim.amount_out
        );

        assert_eq!(
            signal.surplus,
            calculate_surplus(&expected_slow_sim, &expected_fast_sim).unwrap()
        );
        assert_eq!(
            signal.expected_profit,
            calculate_expected_profits(
                &expected_slow_sim,
                &expected_fast_sim,
                strategy.max_slippage_bps,
                strategy.congestion_risk_discount_bps
            )
            .unwrap()
        )
    }

    #[test]
    fn generate_signal_different_decimals_bab() {
        let strategy = make_different_decimals_strategy();

        let slow_state = make_single_univ2_pair_state(
            &strategy.slow_pair,
            2000,
            "0x123",
            5_000,
            10_000,
            tycho_common::models::Chain::Ethereum,
        );

        let fast_state = make_single_univ2_pair_state(
            &strategy.fast_pair,
            100,
            "0x456",
            2_000,
            10_000,
            tycho_common::models::Chain::Base,
        );

        let precompute = strategy.precompute(slow_state);
        let fast_sorted_spot_prices = make_sorted_spot_prices(&fast_state, &strategy.fast_pair);
        let signal = strategy
            .generate_signal(&precompute, fast_state.clone(), fast_sorted_spot_prices)
            .unwrap();

        assert_eq!(signal.slow_pool_id, state::PoolId::from("0x123"));
        assert_eq!(signal.fast_pool_id, state::PoolId::from("0x456"));

        // assert pepe->weth and weth->pepe legs
        assert_eq!(signal.slow_swap_sim.token_in, make_mainnet_weth());
        assert_eq!(signal.slow_swap_sim.token_out, make_mainnet_usdc());
        assert_eq!(signal.fast_swap_sim.token_in, make_base_usdc());
        assert_eq!(signal.fast_swap_sim.token_out, make_base_weth());

        let expected_slow_sim = precompute
            .pool_sims
            .get(&PoolId::from("0x123"))
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
            "0x456",
            expected_fast_amount_in,
            &make_base_pepe(),
            &make_base_weth(),
            fast_state,
        );
        assert_eq!(
            signal.fast_swap_sim.amount_out,
            expected_fast_sim.amount_out
        );

        assert_eq!(
            signal.surplus,
            calculate_surplus(&expected_slow_sim, &expected_fast_sim).unwrap()
        );
        assert_eq!(
            signal.expected_profit,
            calculate_expected_profits(
                &expected_slow_sim,
                &expected_fast_sim,
                strategy.max_slippage_bps,
                strategy.congestion_risk_discount_bps
            )
            .unwrap()
        )
    }
}
