use std::{
    collections::{HashMap, HashSet},
    iter::zip,
};

use color_eyre::eyre::{self, Context};
use num_bigint::BigUint;
use tracing::{debug, error, instrument};
use tycho_common::simulation::protocol_sim::ProtocolSim;

use crate::{
    chain::Chain,
    signals::{Direction, Signal, SimulationResult},
    state::{
        self,
        pair::{Pair, PairState},
    },
};

// Implementation of the arbitrage strategy
pub struct CrossChainSingleHop {
    pub slow_pair: Pair,
    pub slow_chain: Chain,
    pub fast_pair: Pair,
    pub fast_chain: Chain,
    pub min_profit_threshold: f64,
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

    pub(crate) fn generate_signal(
        &self,
        precompute: Precomputes,
        fast_state: PairState,
    ) -> Option<Signal> {
        // 1. find the first crossing pools from precompute & fast_state
        let fast_sorted_spot_prices = (
            make_sorted_spot_prices(&fast_state, &self.fast_pair, Direction::AtoB),
            make_sorted_spot_prices(&fast_state, &self.fast_pair, Direction::BtoA),
        );

        // pools with the best A-> B (slow) and B-> A (fast) trades
        let aba = find_crossed_pools(
            &precompute.sorted_spot_prices.0,
            &fast_sorted_spot_prices.1,
            Direction::AtoB,
        );
        if let Some(aba) = aba {
            debug!(
                slow.chain = %self.slow_chain,
                slow.pool_id = %aba.0,
                fast.chain = %self.fast_chain,
                fast.pool_id = %aba.1,
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

        None
    }
}

#[derive(Debug)]
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

        // TODO: safe math
        // TODO: determine max trade amount based on limits and inventory. min(self.max_protocol_limit * state.get_limits(), self.max_inventory)
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
    sims: HashMap<state::Id, (PoolPrecomputes, PoolPrecomputes)>,
    sorted_spot_prices: (Vec<(state::Id, f64)>, Vec<(state::Id, f64)>),
}

impl Precomputes {
    // TODO: maybe turn this func into async to parallelize the simulations?
    #[instrument(skip_all, fields(
        block.height = %state.block_height,
        pair = %pair,
        with_unmodified_precomputes = %prev_precomputes.is_some(),
    ))]
    pub fn from_pair_state(
        state: PairState,
        pair: &Pair,
        inventory: &(BigUint, BigUint),
        prev_precomputes: Option<Precomputes>,
        steps: usize,
    ) -> Self {
        let mut sims = HashMap::new();

        // reuse precomputes for unmodified pools
        if let Some(prev_precompute) = prev_precomputes {
            // TODO: maybe take this out and just keep the previous signals around in the run function and then feed them into generate_signal
            sims.extend(get_unmodified_precomputes(
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

        sims.extend(precomputes);

        let spot_prices_a_to_b_sorted: Vec<(state::Id, f64)> =
            make_sorted_spot_prices(&state, &pair, Direction::AtoB);
        let spot_prices_b_to_a_sorted: Vec<(state::Id, f64)> =
            make_sorted_spot_prices(&state, &pair, Direction::BtoA);

        Self {
            sims,
            sorted_spot_prices: (spot_prices_a_to_b_sorted, spot_prices_b_to_a_sorted),
        }
    }

    pub fn generate_best_signal(self, fast_state: PairState) -> Signal {
        // TODO: this should take in a crossed_fast_pools: &HashMap<state::Id, Arc<dyn ProtocolSim>>
        // TODO: this should live in the strategy?
        // let best = {
        //     for (pool_id, (a_to_b, b_to_a)) in self.sims {
        //         // let best_a_to_b = a_to_b.best_signal()
        //     }
        // };

        todo!()
    }
}

fn get_unmodified_precomputes(
    mut precomputes: Precomputes,
    unmodified_pools: &HashSet<state::Id>,
) -> HashMap<state::Id, (PoolPrecomputes, PoolPrecomputes)> {
    unmodified_pools
        .iter()
        .filter_map(|pool_id| {
            let (a_to_b, b_to_a) = precomputes.sims.remove(pool_id)?;
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
fn find_crossed_pools(
    slow_prices: &Vec<(state::Id, f64)>,
    fast_prices: &Vec<(state::Id, f64)>,
    slow_direction: Direction,
) -> Option<(state::Id, state::Id, f64)> {
    if slow_prices.is_empty() || fast_prices.is_empty() {
        return None;
    }

    // need to find the max spread
    // spread between two prices is the difference between the two prices
    // slow_in = 1
    // slow_out = slow_price
    // fast_in = slow_price * slow_in = slow_price
    // fast_out = fast_price * fast_in = fast_price * slow_price
    // diff = fast_price * slow_price - slow_price
    //
    // slow[slow.len()] and fast[fast.len()] should have the biggest spread
    // or slow.last() and fast[i] such that i is the highest index that is still <= slow.last()
    // if cant find i for slow.last, look for slow[slow.len() - 1] and so on?
    let mut max_spread = 0.0;
    let mut max_spread_pair = None;

    for (slow_id, slow_price) in slow_prices {
        for (fast_id, fast_price) in fast_prices {
            let spread = (slow_price - fast_price).abs();
            if spread > max_spread {
                max_spread = spread;
                max_spread_pair = Some((*slow_id, *fast_id, spread));
            }
        }
    }

    max_spread_pair
}
