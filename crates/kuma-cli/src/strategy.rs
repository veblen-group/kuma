use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use color_eyre::eyre::{self, Context, ContextCompat, eyre};
use num_bigint::BigUint;
use tracing::{debug, error, instrument, trace};
use tycho_common::simulation::protocol_sim::ProtocolSim;
use tycho_simulation::protocol::models::ProtocolComponent;

use crate::{
    chain::Chain,
    signals::{self, Direction, SimulationResult},
    state::{
        self,
        pair::{Pair, PairState},
    },
};

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
        let fast_sorted_spot_prices = (
            make_sorted_spot_prices(&fast_state, &self.fast_pair, Direction::AtoB),
            make_sorted_spot_prices(&fast_state, &self.fast_pair, Direction::BtoA),
        );

        // pools with the best A-> B (slow) and B-> A (fast) trades
        let aba = find_first_crossed_pools(
            &precompute.sorted_spot_prices.0,
            &fast_sorted_spot_prices.1,
            Direction::AtoB,
        );
        if let Some(aba) = aba.as_ref() {
            debug!(
                slow.chain = %self.slow_chain,
                slow.pool_id = %aba.0,
                fast.chain = %self.fast_chain,
                fast.pool_id = %aba.1,
                spread = %aba.2,
                "found A-> B (slow) and B-> A (fast) trades"
            );
        } else {
            debug!(
                slow.chain = %self.slow_chain,
                fast.chain = %self.fast_chain,
                "no A-> B (slow) and B-> A (fast) trades"
            )
        };

        // TODO: other direction

        // 2. binary search over swap amounts
        if let Some((slow_id, fast_id, _spread)) = aba.as_ref() {
            let precomputes = precompute.pool_sims[slow_id].0.to_owned();
            let (slow_sim, fast_sim) = find_optimal_swap(
                precomputes,
                fast_state.states[fast_id].as_ref(),
                &self.available_inventory_fast.1,
            )
            .wrap_err("unable to find optimal swap")?;

            // TODO: clean up this stuff
            signals::CrossChainSingleHop::try_from_simulations(
                self.slow_chain.clone(),
                self.slow_pair.clone(),
                slow_id.clone(),
                precompute.block_height,
                slow_sim,
                self.fast_chain.clone(),
                self.fast_pair.clone(),
                fast_id.clone(),
                fast_state.block_height,
                fast_sim,
                self.max_slippage_bps,
                self.congestion_risk_discount_bps,
            )
        } else {
            Err(eyre::eyre!("no A-> B (slow) and B-> A (fast) trades"))
        }
    }
}

#[derive(Debug, Clone)]
pub struct PoolPrecomputes {
    // TODO: maybe get rid of it
    pub direction: Direction,
    pub sims: Vec<SimulationResult>,
}

impl PoolPrecomputes {
    #[instrument(skip(protocol_sim, steps))]
    pub fn from_protocol_sim(
        pair: &Pair,
        direction: Direction,
        steps: usize,
        inventory: &BigUint,
        protocol_sim: &dyn ProtocolSim,
    ) -> eyre::Result<Self> {
        let mut sims = vec![];

        if steps == 0 {
            return Err(eyre!("steps must be greater than 0. {:} provided", steps));
        }
        // TODO: determine max trade amount based on limits and inventory:
        // min(max_protocol_limit * state.get_limits(), self.max_inventory)
        let step = inventory / steps;

        for i in 0..=steps {
            let amount_in = &step * i;

            let sim = SimulationResult::from_protocol_sim(
                &amount_in,
                pair.token_a(),
                pair.token_b(),
                protocol_sim,
            ).with_context(||
                format!(
                    "optimal swap precompute for {:} ({:} direction) failed at intermediate step {:} (amount_in {:})",
                    pair,
                    direction,
                    step,
                    amount_in
                ))?;
            sims.push(sim);
        }

        Ok(Self { sims, direction })
    }
}

pub struct Precomputes {
    block_height: u64,
    // TODO: add inventory and pair info?
    sorted_spot_prices: (Vec<(state::Id, f64)>, Vec<(state::Id, f64)>),
    pool_sims: HashMap<state::Id, (PoolPrecomputes, PoolPrecomputes)>,
    #[allow(dead_code)]
    pool_metadata: HashMap<state::Id, Arc<ProtocolComponent>>,
}

impl Precomputes {
    // TODO: maybe turn this func into async to parallelize the simulations?
    #[instrument(skip_all, fields(
        block.height = %state.block_height,
        pair = %pair,
        inventory = ?inventory,
        with_unmodified_precomputes = %prev_precomputes.is_some(),
    ))]
    pub fn from_pair_state(
        state: PairState,
        pair: &Pair,
        inventory: &(BigUint, BigUint),
        prev_precomputes: Option<Precomputes>,
        steps: usize,
    ) -> Self {
        let block_height = state.block_height;

        let mut pool_sims = HashMap::new();

        // reuse precomputes for unmodified pools
        if let Some(prev_precompute) = prev_precomputes {
            // TODO: maybe take this out and just keep the previous signals around in the run function and then feed them into generate_signal
            pool_sims.extend(get_unmodified_precomputes(
                prev_precompute,
                state.unmodified_pools.as_ref(),
            ));
        }

        // add simulation results for modified pools
        let precomputes = state
            .modified_pools
            .as_ref()
            .iter()
            .filter_map(|pool_id| state.states.get(pool_id).map(|pool| (pool_id, pool)))
            .filter_map(|(pool_id, pool)| {
                match make_pool_precomputes(&pair, steps, &inventory, pool.as_ref()){
                    Ok((a_to_b, b_to_a)) => Some((pool_id.clone(), (a_to_b, b_to_a))),
                    Err(e) => {
                        error!(error = %e, pool.id = %pool_id, pair = %pair, "precompute failed, skipping pool");
                        None
                    }
                }
            });

        pool_sims.extend(precomputes);

        let spot_prices_a_to_b_sorted: Vec<(state::Id, f64)> =
            make_sorted_spot_prices(&state, &pair, Direction::AtoB);
        let spot_prices_b_to_a_sorted: Vec<(state::Id, f64)> =
            make_sorted_spot_prices(&state, &pair, Direction::BtoA);

        Self {
            block_height,
            pool_sims,
            sorted_spot_prices: (spot_prices_a_to_b_sorted, spot_prices_b_to_a_sorted),
            pool_metadata: state.metadata,
        }
    }
}

fn get_unmodified_precomputes(
    mut precomputes: Precomputes,
    unmodified_pools: &HashSet<state::Id>,
) -> HashMap<state::Id, (PoolPrecomputes, PoolPrecomputes)> {
    unmodified_pools
        .iter()
        .filter_map(|pool_id| {
            let (a_to_b, b_to_a) = precomputes.pool_sims.remove(pool_id)?;
            Some((pool_id.clone(), (a_to_b, b_to_a)))
        })
        .collect()
}

fn make_pool_precomputes(
    pair: &Pair,
    steps: usize,
    inventory: &(BigUint, BigUint),
    pool: &dyn ProtocolSim,
) -> eyre::Result<(PoolPrecomputes, PoolPrecomputes)> {
    let a_to_b =
        PoolPrecomputes::from_protocol_sim(pair, Direction::AtoB, steps, &inventory.0, pool)
            .wrap_err("failed to precompute a->b direction")?;
    let b_to_a =
        PoolPrecomputes::from_protocol_sim(pair, Direction::BtoA, steps, &inventory.1, pool)
            .wrap_err("failed to precompute b->a direction")?;

    Ok((a_to_b, b_to_a))
}

fn make_sorted_spot_prices(
    state: &PairState,
    pair: &Pair,
    direction: Direction,
) -> Vec<(state::Id, f64)> {
    let mut spots: Vec<(state::Id, f64)> = state
        .states
        .iter()
        .filter_map(|(id, pool)| {
            let spot_price = match direction {
                Direction::AtoB => pool.spot_price(pair.token_a(), pair.token_b()),
                Direction::BtoA => pool.spot_price(pair.token_b(), pair.token_a()),
            };
            match spot_price {
                Ok(price) => Some((id.clone(), price)),
                Err(err) => {
                    debug!(
                        error = %err,
                        pair = %pair,
                        direction = %direction,
                        "failed to get spot price, skipping pool"
                    );
                    None
                }
            }
        })
        .collect();

    spots.sort_by(|(_, spot_price), (_, other_spot_price)| spot_price.total_cmp(other_spot_price));
    spots
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
    sorted_slow_prices: &[(state::Id, f64)],
    sorted_fast_prices: &[(state::Id, f64)],
    slow_direction: Direction,
) -> Option<(state::Id, state::Id, f64)> {
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
            debug!(slow_price = %slow_price, "searching for fast chain pool with crossing price");
            sorted_fast_prices.iter().find_map(|(fast_id, fast_price)| {
                let spread = fast_price - slow_price;
                if spread > 0.0 {
                    Some((slow_id.clone(), fast_id.clone(), spread))
                } else {
                    None
                }
            })
        })
}

#[instrument(skip_all, fields(
    // TODO fix args to provide this?
    direction = %precomputes.direction,
    // slow.block_height = %precompute.block_height,
    // slow.pool_id = %precompute.id(),
    // fast.block_height = %fast_state.block_height,
    // fast.pool_id = %fast_state.id(),
    fast.inventory = %fast_inventory,
))]
fn find_optimal_swap(
    // TODO: precomputes id, block_height, inventory?
    precomputes: PoolPrecomputes,
    // TODO: add fast id, block_height?
    fast_state: &dyn ProtocolSim,
    fast_inventory: &BigUint,
) -> eyre::Result<(SimulationResult, SimulationResult)> {
    let (mut left, mut right) = (0, precomputes.sims.len());

    let mut best_sims: Option<(SimulationResult, SimulationResult)> = None;

    while left < right {
        let mid = left + (right - left) / 2;
        let slow_sim = precomputes.sims[mid].to_owned();

        // TODO: add slippage into calculation
        let (fast_in, fast_out) = (&slow_sim.token_out, &slow_sim.token_in);
        let amount_in = fast_inventory.min(&slow_sim.amount_out);

        let fast_sim =
            SimulationResult::from_protocol_sim(amount_in, fast_in, fast_out, fast_state)?;

        match best_sims.as_ref() {
            Some(best) if fast_sim.amount_out > best.0.amount_out => {
                best_sims = Some((slow_sim, fast_sim));
                // found new best, search right half
                left = mid + 1;
            }
            None => {
                // found first best, search left half
                best_sims = Some((slow_sim, fast_sim));
                right = mid - 1;
            }
            Some(best) => {
                trace!(best_sim.amount_out = %best.0.amount_out, new_sim.amount_out = %fast_sim.amount_out, "tried sim amount but less than best")
            }
        }
    }

    best_sims.wrap_err("no optimal swap found")
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
        sync::Arc,
    };
    use tycho_common::models::token::Token;
    use tycho_common::simulation::protocol_sim::ProtocolSim;

    // Helper function to create UniswapV2State for testing
    fn create_uniswap_v2_state_with_liquidity(
        reserve_a: u64,
        reserve_b: u64,
    ) -> Arc<dyn ProtocolSim> {
        use std::str::FromStr;
        use tycho_simulation::evm::protocol::uniswap_v2::state::UniswapV2State;

        let reserve_a_u256 = alloy::primitives::U256::from_str(&reserve_a.to_string()).unwrap();
        let reserve_b_u256 = alloy::primitives::U256::from_str(&reserve_b.to_string()).unwrap();

        Arc::new(UniswapV2State::new(reserve_a_u256, reserve_b_u256))
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

    fn make_default_strategy() -> Arc<strategy::CrossChainSingleHop> {
        let slow_chain = Chain::eth_mainnet();
        let fast_chain = Chain::base_mainnet();

        Arc::new(CrossChainSingleHop {
            slow_chain,
            slow_pair: Pair::new(make_mainnet_usdc(), make_mainnet_weth()),
            available_inventory_slow: (BigUint::from(1000u64), BigUint::from(500u64)),
            fast_chain,
            fast_pair: Pair::new(make_base_usdc(), make_base_weth()),
            available_inventory_fast: (BigUint::from(1200u64), BigUint::from(600u64)),
            max_slippage_bps: 25, // 0.25%
            congestion_risk_discount_bps: 25,
            // min_profit_threshold: 0.5, // 0.5%
            binary_search_steps: 5,
        })
    }

    fn make_single_univ2_pool_state(
        block_height: u64,
        pool_id: &str,
        reserve_a: u64,
        reserve_b: u64,
    ) -> PairState {
        PairState {
            states: HashMap::from([(
                state::Id::from(pool_id),
                create_uniswap_v2_state_with_liquidity(reserve_a, reserve_b),
            )]),
            block_height,
            modified_pools: Arc::new(HashSet::from([state::Id::from("0x123")])),
            unmodified_pools: Arc::new(HashSet::new()),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn precompute_sanity_check() {
        // TODO: import helpers for state and metadata from signals tests
        // TODO: instantiate expected precompute and compare the objects
        // assert_eq!(precompute.calculations.len(), 10); // 5 steps × 2 paths
        // assert_eq!(precompute.chain, slow_chain);

        // // Verify precompute calculations
        // let first_calc = &precompute.calculations[0];
        // assert_eq!(first_calc.amount_in, BigUint::from(200u64)); // 1000 / 5 steps
        // assert!(matches!(first_calc.path, Direction::AtoB)); // First path should be AtoB

        // let second_calc = &precompute.calculations[1];
        // assert_eq!(second_calc.amount_in, BigUint::from(200u64));
        // assert!(matches!(second_calc.path, Direction::BtoA)); // Second path should be BtoA
        let strategy = make_default_strategy();

        // Create slow state (favors token B)
        // 0x123 -> univ2(950k, 1m)
        let slow_state = make_single_univ2_pool_state(2000, "0x123", 950_000, 1_000_000);

        let precompute = strategy.precompute(slow_state);
        assert_eq!(precompute.block_height, 2000);

        // TODO: fix
        // correct spot prices
        assert_eq!(
            precompute.sorted_spot_prices.0[0],
            (state::Id::from("0x123"), 950_000f64)
        );
        assert_eq!(
            precompute.sorted_spot_prices.0[0],
            (state::Id::from("0x123"), 950_000f64)
        );

        // only one pool simulated
        assert_eq!(precompute.pool_sims.len(), 1);
        // all steps for binary search have been precomputed in both directions
        assert_eq!(
            precompute.pool_sims[&state::Id::from("0x123")].0.sims.len(),
            strategy.binary_search_steps
        );
        assert_eq!(
            precompute.pool_sims[&state::Id::from("0x123")].1.sims.len(),
            strategy.binary_search_steps
        );

        // start at 0 and end at inventory
        assert_eq!(
            precompute.pool_sims[&state::Id::from("0x123")].0.sims[0].amount_in,
            BigUint::from(0u64)
        );
        assert_eq!(
            precompute.pool_sims[&state::Id::from("0x123")].0.sims[0].amount_out,
            BigUint::from(0u64)
        );

        assert_eq!(
            precompute.pool_sims[&state::Id::from("0x123")].0.sims
                [strategy.binary_search_steps - 1]
                .amount_in,
            BigUint::from(strategy.available_inventory_slow.0.clone())
        );
        assert_eq!(
            precompute.pool_sims[&state::Id::from("0x123")].0.sims
                [strategy.binary_search_steps - 1]
                .amount_out,
            // TODO: fix
            BigUint::from(strategy.available_inventory_slow.0.clone())
        );
    }

    #[test]
    fn compute_arb_sanity_check() {
        let strategy = make_default_strategy();

        // Create slow state (favors token B)
        // 0x123 -> univ2(950k, 1m)
        let slow_state = make_single_univ2_pool_state(2000, "0x123", 950_000, 1_000_000);

        // Create fast state (favors token A)
        // 0x456 -> univ2(1m, 950k)
        let fast_state = make_single_univ2_pool_state(100, "0x456", 1_000_000, 950_000);

        let precompute = strategy.precompute(slow_state);
        let signal = strategy.generate_signal(precompute, fast_state).unwrap();

        assert_eq!(signal.expected_profit, BigUint::from(0u64));
        assert_eq!(signal.slow_id, state::Id::from("0x123"));
        assert_eq!(signal.fast_id, state::Id::from("0x456"));

        assert_eq!(signal.slow_sim.token_in, make_mainnet_weth());
        assert_eq!(signal.slow_sim.token_out, make_mainnet_usdc());
        assert_eq!(signal.fast_sim.token_in, make_base_usdc());
        assert_eq!(signal.fast_sim.token_out, make_base_weth());
    }
}
