use color_eyre::eyre::{self, Context, OptionExt};
use num_bigint::BigUint;
use tracing::{debug, instrument, trace};
use tycho_common::simulation::protocol_sim::ProtocolSim;

use crate::{
    chain::Chain,
    signals::{self, Direction},
    state::{
        self, PoolId,
        pair::{Pair, PairState},
    },
    strategy::{precompute::Precomputes, simulation::make_sorted_spot_prices},
};

mod precompute;
mod simulation;
pub use simulation::Swap;

// Implementation of the arbitrage strategy
// TODO: should this and precompute be different types or should this just populate
#[derive(Debug)]
pub struct CrossChainSingleHop {
    // TODO: make a (chain, pair, inventory) tuple?
    pub slow_pair: Pair,
    pub slow_chain: Chain,
    pub fast_pair: Pair,
    pub fast_chain: Chain,
    pub available_inventory_slow: (BigUint, BigUint),
    pub available_inventory_fast: (BigUint, BigUint),
    pub binary_search_steps: usize,
    pub max_slippage_bps: u64,
    pub congestion_risk_discount_bps: u64,
}

impl CrossChainSingleHop {
    #[instrument(skip_all)]
    pub fn precompute(&self, slow_state: PairState) -> Precomputes {
        Precomputes::from_pair_state(
            slow_state,
            &self.slow_pair,
            &self.available_inventory_slow,
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
    pub(crate) fn generate_signal(
        &self,
        precompute: Precomputes,
        fast_state: PairState,
    ) -> eyre::Result<signals::CrossChainSingleHop> {
        // 1. find the first pair of crossing pools from precompute & fast_state
        // TODO: for the other direction we need slow_b_to_a and fast_a_to_b
        let fast_sorted_spot_prices = {
            let a_to_b = make_sorted_spot_prices(&fast_state, &self.fast_pair, Direction::AtoB);
            let b_to_a = make_sorted_spot_prices(&fast_state, &self.fast_pair, Direction::BtoA);
            (a_to_b, b_to_a)
        };

        // pools with the best A-> B (slow) and B-> A (fast) trades
        let aba = find_first_crossed_pools(
            &precompute.sorted_spot_prices.0,
            &fast_sorted_spot_prices.1,
            Direction::AtoB,
        );
        if let Some(aba) = aba.as_ref() {
            trace!(
                slow.pool_id = %aba.0,
                fast.pool_id = %aba.1,
                spread = %aba.2,
                "found A-> B and B-> A crossing pools"
            );
        } else {
            trace!("no A-> B and B-> A crossing pools")
        };

        // TODO: other direction

        // 2. binary search over swap amounts
        if let Some((slow_id, fast_id, _spread)) = aba.as_ref() {
            let signal = self
                .find_optimal_signal(
                    &precompute.pool_sims[slow_id].b_to_a,
                    &slow_id,
                    precompute.block_height,
                    fast_state.states[fast_id].as_ref(),
                    &fast_id,
                    fast_state.block_height,
                    &self.available_inventory_fast.1,
                )
                .wrap_err("unable to find optimal swap")?;

            trace!(slow_sim = %signal.slow_sim, fast_sim = %signal.fast_sim, "found optimal swap");
            Ok(signal)
        } else {
            Err(eyre::eyre!("no A-> B (slow) and B-> A (fast) trades"))
        }

        // TODO: other direction
        // TODO: compare aba and bab signals
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
    // TODO: add slow_inventory to logs?
    #[instrument(skip(self, slow_pool_sims, fast_state))]
    fn find_optimal_signal(
        &self,
        // TODO: have an abstraction around slow = (height, pool_id, sims) and fast = (height, pool_id, protocol_sim, inventory)
        slow_pool_sims: &Vec<Swap>,
        slow_pool_id: &PoolId,
        slow_height: u64,
        fast_state: &dyn ProtocolSim,
        fast_pool_id: &PoolId,
        fast_height: u64,
        fast_inventory: &BigUint,
    ) -> eyre::Result<signals::CrossChainSingleHop> {
        let (mut left, mut right) = (0, slow_pool_sims.len());

        let mut best_signal: Option<signals::CrossChainSingleHop> = None;

        while left < right {
            let mid = (right + left) / 2;

            // make sims for mid
            let mid_slow_sim = slow_pool_sims[mid].clone();
            let mid_fast_sim =
                self.swap_from_precompute(slow_pool_sims[mid].clone(), fast_state, fast_inventory)?;

            let mid_signal = match self.try_signal_from_swap(
                mid_slow_sim.clone(),
                slow_pool_id,
                slow_height,
                mid_fast_sim.clone(),
                fast_pool_id,
                fast_height,
            ) {
                Ok(signal) => signal,
                Err(err) => {
                    trace!(err = %err, slow_sim = %mid_slow_sim, fast_sim = %mid_fast_sim, index = mid, "failed to make signal out of simulation results");
                    // TODO: exit early?
                    continue;
                }
            };

            // make sims for next
            let next_slow_sim = slow_pool_sims[mid + 1].clone();
            let next_fast_sim = self.swap_from_precompute(
                slow_pool_sims[mid + 1].clone(),
                fast_state,
                fast_inventory,
            )?;
            let next_signal = match self.try_signal_from_swap(
                next_slow_sim.clone(),
                slow_pool_id,
                slow_height,
                next_fast_sim.clone(),
                fast_pool_id,
                fast_height,
            ) {
                Ok(signal) => signal,
                Err(err) => {
                    trace!(err = %err, slow_sim = %next_slow_sim, fast_sim = %next_fast_sim, index = mid+1, "failed to make signal out of simulation results");
                    // TODO: exit early?
                    continue;
                }
            };

            // compare the expected profits
            // TODO: is this the correct value to compare?
            // TODO: move this out to a function that compares two signals?
            if mid_signal.expected_profit < next_signal.expected_profit {
                // next is higher -> check to the right (try a higher amount_in)
                left = mid;
                trace!(index = mid, curr = %mid_signal, next= %next_signal, "next signal has higher expected profit, continuing search");
                best_signal = Some(next_signal);
            } else {
                // next is lower -> check to the left (try a lower amount_in)
                trace!(index = mid, curr = %mid_signal, next= %next_signal, "next signal has higher expected profit, continuing search");
                right = mid;
            }
        }

        trace!(found_signal = %best_signal.is_some(), "search complete");

        best_signal.ok_or_eyre("unable to find signal with non-negative expected profit")
    }

    /// This creates the fast leg of the arbitrage out of the precompute slow leg.
    fn swap_from_precompute(
        &self,
        precompute: simulation::Swap,
        fast_state: &dyn ProtocolSim,
        fast_inventory: &BigUint,
    ) -> eyre::Result<simulation::Swap> {
        if fast_inventory < &precompute.amount_out {
            return Err(eyre::eyre!("fast inventory is insufficient"));
        }
        Swap::from_protocol_sim(
            &precompute.amount_out,
            &precompute.token_out,
            &precompute.token_in,
            fast_state,
        )
        .wrap_err("failed to compute fast simulation for next")
    }

    fn try_signal_from_swap(
        &self,
        slow_sim: simulation::Swap,
        slow_pool_id: &PoolId,
        slow_height: u64,
        fast_sim: simulation::Swap,
        fast_pool_id: &PoolId,
        fast_height: u64,
    ) -> eyre::Result<signals::CrossChainSingleHop> {
        signals::CrossChainSingleHop::try_from_simulations(
            &self.slow_chain,
            &self.slow_pair,
            slow_pool_id,
            slow_height,
            slow_sim,
            &self.fast_chain,
            &self.fast_pair,
            fast_pool_id,
            fast_height,
            fast_sim,
            self.max_slippage_bps,
            self.congestion_risk_discount_bps,
        )
        .wrap_err("failed to make candidate signal")
    }
}

/// Finds the pair of pools with the biggest difference in spot prices based
/// on the provided direction. The direction denotes the trade direction on the
/// slow chain.
///
/// slow_prices contain the A -> B prices on the slow chain, sorted from lowest to highest.
/// fast_prices contain the B -> A prices on the fast chain, sorted from lowest to highest.
///
/// # Returns
/// A tuple of pool IDs (slow_id, fast_id, spread) denoting the pool IDs corresponding to the
/// slow and fast chains respectively, and the spread between the two prices.
#[instrument]
fn find_first_crossed_pools(
    sorted_slow_prices: &[(state::PoolId, f64)],
    sorted_fast_prices: &[(state::PoolId, f64)],
    slow_direction: Direction,
) -> Option<(state::PoolId, state::PoolId, f64)> {
    if sorted_slow_prices.is_empty() || sorted_fast_prices.is_empty() {
        return None;
    }
    // need to find the max spread
    // because the spot prices are sorted, we can start from the highest slow price
    // and the lowest fast price, iterating backwards over slow prices and forwards over fast prices:
    // slow:   [1, 2, 3]
    // spread:  ↱ =2  ↲  <- highest spread
    // fast:   [1, 2, 3]
    // TODO: do this with binary search instead?
    sorted_slow_prices
        .iter()
        .rev()
        .find_map(|(slow_id, slow_price)| {
            trace!(slow_price = %slow_price, "searching for fast chain pool with crossing price");
            sorted_fast_prices.iter().find_map(|(fast_id, fast_price)| {
                let spread = fast_price - slow_price;
                if spread > 0.0 {
                    debug!(slow_price = %slow_price, fast_price = %fast_price, spread = %spread, "found crossing price");
                    Some((slow_id.clone(), fast_id.clone(), spread))
                } else {
                    None
                }
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        chain::Chain,
        state::{self, pair::PairState},
        strategy::{self, CrossChainSingleHop},
    };
    use std::{
        collections::{HashMap, HashSet},
        str::FromStr as _,
        sync::{Arc, OnceLock},
    };
    use tracing_subscriber::EnvFilter;
    use tycho_common::models::token::Token;
    use tycho_common::simulation::protocol_sim::ProtocolSim;

    static TELEMETRY_INIT: OnceLock<()> = OnceLock::new();

    fn init_tracing() {
        TELEMETRY_INIT.get_or_init(|| {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::from_default_env())
                .with_thread_names(true)
                .pretty()
                .with_line_number(true)
                .with_test_writer()
                .try_init();
        });
    }

    fn make_token(symbol: &str, chain: tycho_common::models::Chain, decimals: u32) -> Token {
        Token::new(
            &tycho_common::Bytes::from_str("0x0000000000000000000000000000000000000000").unwrap(),
            symbol,
            decimals,
            1000,
            &[Some(1000u64)],
            chain,
            100,
        )
    }

    fn make_mainnet_weth() -> Token {
        Token::new(
            &tycho_common::Bytes::from_str("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2").unwrap(),
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
            &tycho_common::Bytes::from_str("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap(),
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
            &tycho_common::Bytes::from_str("0x4200000000000000000000000000000000000006").unwrap(),
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
            &tycho_common::Bytes::from_str("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913").unwrap(),
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

    fn make_same_decimals_strategy() -> Arc<strategy::CrossChainSingleHop> {
        init_tracing();

        let slow_chain = Chain::eth_mainnet();
        let slow_pair = Pair::new(make_token("PEPE", slow_chain.name, 18), make_mainnet_weth());
        let available_inventory_slow = (
            scale_by_decimals(&BigUint::from(100_000u64), slow_pair.token_a().decimals),
            scale_by_decimals(&BigUint::from(50u64), slow_pair.token_b().decimals),
        );

        let fast_chain = Chain::base_mainnet();
        let fast_pair = Pair::new(make_token("PEPE", fast_chain.name, 18), make_base_weth());
        let available_inventory_fast = (
            scale_by_decimals(&BigUint::from(200_000u64), fast_pair.token_a().decimals),
            scale_by_decimals(&BigUint::from(40u64), fast_pair.token_b().decimals),
        );

        Arc::new(CrossChainSingleHop {
            slow_chain,
            slow_pair,
            available_inventory_slow,
            fast_chain,
            fast_pair,
            available_inventory_fast,
            max_slippage_bps: 25, // 0.25%
            congestion_risk_discount_bps: 25,
            // min_profit_threshold: 0.5, // 0.5%
            binary_search_steps: 5,
        })
    }

    fn make_univ2_protocol_sim(reserve_a: &BigUint, reserve_b: &BigUint) -> Arc<dyn ProtocolSim> {
        use std::str::FromStr;
        use tycho_simulation::evm::protocol::uniswap_v2::state::UniswapV2State;

        let reserve_a_u256 = alloy::primitives::U256::from_str(&reserve_a.to_string()).unwrap();
        let reserve_b_u256 = alloy::primitives::U256::from_str(&reserve_b.to_string()).unwrap();

        Arc::new(UniswapV2State::new(reserve_a_u256, reserve_b_u256))
    }

    fn make_single_univ2_pool_state(
        pair: &Pair,
        block_height: u64,
        pool_id: &str,
        reserve_a: u64,
        reserve_b: u64,
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
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn precompute_same_decimals_sanity_check() {
        // Arrange
        // slow chain inventory is 100,000 PEPE and 50 ETH
        let strategy = make_same_decimals_strategy();

        // 0x123 -> univ2(1m, 1k)
        // spot price should be ~1000/ or 0.001
        let slow_state =
            make_single_univ2_pool_state(&strategy.slow_pair, 0, "0x123", 1_000_000, 1_000);

        // Act
        let precompute = strategy.precompute(slow_state.clone());
        assert_eq!(precompute.block_height, 0);

        // Assert
        // correct spot prices
        assert_eq!(
            precompute.sorted_spot_prices.0[0],
            (state::PoolId::from("0x123"), "0.001".parse().unwrap())
        );
        assert_eq!(
            precompute.sorted_spot_prices.1[0],
            (state::PoolId::from("0x123"), "1000".parse().unwrap())
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
            scale_by_decimals(
                &BigUint::from(20_000u64),
                strategy.slow_pair.token_a().decimals
            )
        );
        // 50 ETH / 5 steps = 10 ETH
        let first_b_to_a = &precompute.pool_sims[&state::PoolId::from("0x123")].b_to_a[0];
        assert_eq!(
            first_b_to_a.amount_in,
            scale_by_decimals(&BigUint::from(10u64), strategy.slow_pair.token_b().decimals)
        );

        // check valid last step inputs
        // 100,000 PEPE
        let last_amount_in_a = &precompute.pool_sims[&state::PoolId::from("0x123")].a_to_b
            [strategy.binary_search_steps - 1]
            .amount_in;
        assert_eq!(
            *last_amount_in_a,
            scale_by_decimals(
                &BigUint::from(100_000u64),
                strategy.slow_pair.token_a().decimals
            )
        );

        // 50 ETH
        let last_amount_in_b = &precompute.pool_sims[&state::PoolId::from("0x123")].b_to_a
            [strategy.binary_search_steps - 1]
            .amount_in;
        assert_eq!(
            *last_amount_in_b,
            scale_by_decimals(&BigUint::from(50u64), strategy.slow_pair.token_b().decimals)
        );
    }

    #[test]
    fn precompute_different_decimals_sanity_check() {}

    #[test]
    fn generate_signal_same_decimals_sanity_check() {
        let strategy = make_same_decimals_strategy();

        // Create slow state
        // 0x123 -> univ2(1m, 1k)
        // spot price should be ~1000 or 0.001
        let slow_state =
            make_single_univ2_pool_state(&strategy.slow_pair, 2000, "0x123", 1_000_000, 1_100_000);

        // Create fast state
        // 0x456 -> univ2(1.25m, 1k)
        // spot price should be ~1250 or 0.0008
        let fast_state =
            make_single_univ2_pool_state(&strategy.fast_pair, 100, "0x456", 1_250_000, 1_000_000);

        let precompute = strategy.precompute(slow_state);
        let signal = strategy.generate_signal(precompute, fast_state).unwrap();

        // TODO: why is my fast_price 8e20 instead of 8e-4 (its multiplied by an extra 10^(6+18))

        // assert_eq!(signal.expected_profit, BigUint::from(0u64));
        assert_eq!(signal.slow_id, state::PoolId::from("0x123"));
        assert_eq!(signal.fast_id, state::PoolId::from("0x456"));

        assert_eq!(signal.slow_sim.token_in, make_mainnet_weth());
        assert_eq!(signal.slow_sim.token_out, make_mainnet_usdc());
        assert_eq!(signal.fast_sim.token_in, make_base_usdc());
        assert_eq!(signal.fast_sim.token_out, make_base_weth());

        // TODO: check amounts
    }

    #[test]
    fn generate_signal_different_decimals_sanity_check() {}
}
