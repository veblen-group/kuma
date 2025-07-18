use std::collections::{HashMap, HashSet};

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
        // 1. find all crossing pools from precompute & fast

        // 2. for each pair of crossing pools, binary search over swap amounts
        // 3. choose best crossing pair
        // 4. choose best direction (probably can just be part of 3)

        // let signal = precompute.generate_best_signal();

        // let mut best_signal: Option<Signal> = None;
        // let mut best_profit = BigUint::from(0u64);s

        // // TODO: binary search over calculation
        // for slow_calc in &precompute.calculations {
        //     // Complete the arbitrage path based on the slow chain calculation
        //     let pool = fast_state.states.values().next().unwrap().clone();

        //     // Use the amount_out from slow chain as amount_in for fast chain
        //     // TODO: slippage adjustment here? probably a question for marcus/tanay
        //     if let Ok(fast_amount_out) = self.simulate_swap(
        //         pool,
        //         &slow_calc.output_token,
        //         &slow_calc.input_token,
        //         &slow_calc.amount_out,
        //     ) {
        //         // Calculate profit: fast_amount_out - slow_amount_in
        //         if fast_amount_out > slow_calc.amount_in {
        //             let profit = &fast_amount_out - &slow_calc.amount_in;
        //             let profit_percentage = (profit.clone() * BigUint::from(10000u64)
        //                 / &slow_calc.amount_in)
        //                 .to_string()
        //                 .parse::<f64>()
        //                 .unwrap_or(0.0)
        //                 / 100.0;

        //             // TODO: profit % should probably be handled at the signal promotion stage
        //             if profit_percentage >= self.min_profit_threshold && profit > best_profit {
        //                 best_profit = profit.clone();
        //                 best_signal = Some(Signal {
        //                     slow_pair: self.slow_pair.clone(),
        //                     fast_pair: self.fast_pair.clone(),
        //                     slow_chain: precompute.chain.clone(),
        //                     fast_chain: fast_chain.clone(),
        //                     path: slow_calc.path.clone(),
        //                     slow_chain_amount_out: slow_calc.amount_out.clone(),
        //                     fast_chain_amount_out: fast_amount_out,
        //                     profit_percentage,
        //                     optimal_amount_in: slow_calc.amount_in.clone(),
        //                     expected_profit: profit,
        //                 });
        //             }
        //         }
        //     }
        // }

        // best_signal
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
    spot_prices_sorted: (Vec<(state::Id, f64)>, Vec<(state::Id, f64)>),
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
            make_sorted_spot_prices_for_direction(&state, &pair, Direction::AtoB);
        let spot_prices_b_to_a_sorted: Vec<(state::Id, f64)> =
            make_sorted_spot_prices_for_direction(&state, &pair, Direction::BtoA);

        Self {
            sims,
            spot_prices_sorted: (spot_prices_a_to_b_sorted, spot_prices_b_to_a_sorted),
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

fn make_sorted_spot_prices_for_direction(
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
