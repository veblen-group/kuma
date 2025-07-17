use std::{collections::HashMap, sync::Arc};

use num_bigint::BigUint;
use tracing::{debug, trace};
use tycho_common::{models::token::Token, simulation::protocol_sim::ProtocolSim};

use crate::{
    chain::Chain,
    signals::{Signal, SimulationResult},
    state::{
        self,
        pair::{Pair, PairState},
    },
};

// Implementation of the arbitrage strategy
pub struct CrossChainSingleHop {
    pub slow_pair: Pair,
    pub fast_pair: Pair,
    pub min_profit_threshold: f64,
    // TODO: rename to inventory
    pub available_inventory: BigUint,
    pub binary_search_steps: usize,
    // TODO: change to u64
    pub max_slippage_bps: f64,
    pub congestion_risk_discount_bps: u64,
}

impl CrossChainSingleHop {
    pub fn run() {
        // 1. precompute from slow state
        // 2. generate best signal
        // 3. output signal
    }

    pub(crate) fn generate_signal(
        &self,
        precompute: &Precompute,
        fast_state: &PairState,
        fast_chain: &Chain,
    ) -> Option<Signal> {
        let mut best_signal: Option<Signal> = None;
        let mut best_profit = BigUint::from(0u64);

        // TODO: binary search over calculation
        for slow_calc in &precompute.calculations {
            // Complete the arbitrage path based on the slow chain calculation
            let pool = fast_state.states.values().next().unwrap().clone();

            // Use the amount_out from slow chain as amount_in for fast chain
            // TODO: slippage adjustment here? probably a question for marcus/tanay
            if let Ok(fast_amount_out) = self.simulate_swap(
                pool,
                &slow_calc.output_token,
                &slow_calc.input_token,
                &slow_calc.amount_out,
            ) {
                // Calculate profit: fast_amount_out - slow_amount_in
                if fast_amount_out > slow_calc.amount_in {
                    let profit = &fast_amount_out - &slow_calc.amount_in;
                    let profit_percentage = (profit.clone() * BigUint::from(10000u64)
                        / &slow_calc.amount_in)
                        .to_string()
                        .parse::<f64>()
                        .unwrap_or(0.0)
                        / 100.0;

                    // TODO: profit % should probably be handled at the signal promotion stage
                    if profit_percentage >= self.min_profit_threshold && profit > best_profit {
                        best_profit = profit.clone();
                        best_signal = Some(Signal {
                            slow_pair: self.slow_pair.clone(),
                            fast_pair: self.fast_pair.clone(),
                            slow_chain: precompute.chain.clone(),
                            fast_chain: fast_chain.clone(),
                            path: slow_calc.path.clone(),
                            slow_chain_amount_out: slow_calc.amount_out.clone(),
                            fast_chain_amount_out: fast_amount_out,
                            profit_percentage,
                            optimal_amount_in: slow_calc.amount_in.clone(),
                            expected_profit: profit,
                        });
                    }
                }
            }
        }

        best_signal
    }

    fn simulate_swap(
        &self,
        state: Arc<dyn ProtocolSim>,
        token_in: &Token,
        token_out: &Token,
        amount_in: &BigUint,
    ) -> Result<BigUint, String> {
        // Get the swap result using Tycho's simulation
        let swap_result = state
            .get_amount_out(amount_in.clone(), token_in, token_out)
            .map_err(|e| format!("Swap simulation failed: {:?}", e))?;

        let mut min_amount_out =
            with_slippage_tolerance(&swap_result.amount, self.max_slippage_bps);
        min_amount_out = with_risk_factor(&swap_result.amount, self.congestion_risk_discount_bps);

        Ok(min_amount_out)
    }
}
#[derive(Debug)]
pub struct PoolPrecomputes {
    // TODO: calculation struct
    pub a_to_b: Vec<SimulationResult>,
    pub b_to_a: Vec<SimulationResult>,
}

impl PoolPrecomputes {
    pub fn from_protocol_sim(
        pair: &Pair,
        steps: usize,
        inventory: (BigUint, BigUint),
        protocol_sim: &dyn ProtocolSim,
    ) -> Self {
        let mut a_to_bs = vec![];
        let mut b_to_as = vec![];

        // TODO: safe math
        // TODO: determine max trade amount based on limits and inventory. min(self.max_protocol_limit * state.get_limits(), self.max_inventory)
        let a_step = inventory.0 / steps;
        let b_step = inventory.1 / steps;

        for i in 0..=steps {
            let a_in = a_step.clone() * i;
            match SimulationResult::from_protocol_sim(
                a_in.clone(),
                pair.token_a(),
                pair.token_b(),
                protocol_sim,
            ) {
                Ok(a_to_b) => a_to_bs.push(a_to_b),
                Err(e) => {
                    debug!(error=%e, amount=%a_in, step = %i, "optimal swap precompute failed at intermediate step, discarding");
                }
            }

            let b_in = b_step.clone() * i;
            match SimulationResult::from_protocol_sim(
                b_in.clone(),
                pair.token_b(),
                pair.token_a(),
                protocol_sim,
            ) {
                Ok(b_to_a) => b_to_as.push(b_to_a),
                Err(e) => {
                    debug!(error=%e, amount=%b_in, step = %i, "optimal swap precompute failed at intermediate step, discarding");
                    return Self {
                        a_to_b: a_to_bs,
                        b_to_a: b_to_as,
                    };
                }
            }
        }

        Self {
            a_to_b: a_to_bs,
            b_to_a: b_to_as,
        }
    }
}

pub struct Precomputes {
    pools: HashMap<state::Id, PoolPrecomputes>,
}

impl Precomputes {
    // TODO: turn this func into async to parallelize the simulations?
    pub fn from_pair_state(
        state: PairState,
        pair: &Pair,
        inventory: (BigUint, BigUint),
        prev_precomputes: Option<Precomputes>,
        steps: usize,
    ) -> Self {
        let mut pools = HashMap::new();

        // copy over precomputes for unmodified pools
        // TODO: maybe take this out and just keep the previous signals around in the run function and then feed them into generate_signal
        if let Some(mut prev_precomputes) = prev_precomputes {
            for pool_id in state.unmodified_pools.as_ref() {
                if let Some(pool_precomputes) = prev_precomputes.pools.remove(&pool_id) {
                    pools.insert(pool_id.clone(), pool_precomputes);
                } else {
                    trace!(
                        pool.id = %pool_id,
                        "precompute not found for unmodified pool"
                    );
                    // TODO: do i make a new one for it? this shouldn't happen...
                    // probably fine to just ignore
                }
            }
        }

        // add simulation results for modified pools
        for pool_id in state.modified_pools.as_ref() {
            if let Some(pool) = state.states.get(pool_id) {
                let pre = PoolPrecomputes::from_protocol_sim(
                    pair,
                    steps,
                    inventory.clone(),
                    pool.as_ref(),
                );
                pools.insert(pool_id.clone(), pre);
            } else {
                trace!(
                    pool.id = %pool_id,
                    "modified pool not found in state"
                );
            }
        }
        Self { pools }
    }

    pub fn generate_best_signal(self) -> Signal {
        // TODO: this should live in the strategy
        todo!("binary search over simulation results")
    }
}
