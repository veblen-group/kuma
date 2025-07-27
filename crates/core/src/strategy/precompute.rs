use std::{collections::HashMap, sync::Arc};

use num_bigint::BigUint;
use tracing::{error, instrument, trace};
use tycho_simulation::protocol::models::ProtocolComponent;

use crate::{
    signals::Direction,
    state::{
        self, PoolId,
        pair::{Pair, PairState},
    },
    strategy::simulation::{self, make_sorted_spot_prices},
};

#[derive(Debug, Clone)]
pub struct Precomputes {
    pub block_height: u64,
    pub sorted_spot_prices: (Vec<(state::PoolId, f64)>, Vec<(state::PoolId, f64)>),
    pub pool_sims: HashMap<state::PoolId, simulation::PoolSteps>,
    #[allow(dead_code)]
    pool_metadata: HashMap<state::PoolId, Arc<ProtocolComponent>>,
}

impl Precomputes {
    // TODO: maybe turn this func into async to parallelize the simulations?
    #[instrument(skip_all, fields(
        block.height = %state.block_height,
        pair = %pair,
        inventory = ?inventory,
        with_unmodified_precomputes = %unmodified_precomputes.is_some(),
    ))]
    pub fn from_pair_state(
        state: &PairState,
        pair: &Pair,
        inventory: &(BigUint, BigUint),
        unmodified_precomputes: Option<Precomputes>,
        steps: usize,
    ) -> Self {
        let block_height = state.block_height;

        let mut pool_sims = HashMap::new();

        // reuse precomputes for unmodified pools
        if let Some(mut precomputes) = unmodified_precomputes {
            // TODO: maybe take this out and just keep the previous signals around in the run function and then feed them into generate_signal

            let unmodified_sims: HashMap<PoolId, simulation::PoolSteps> = state
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
            .modified_pools
            .as_ref()
            .iter()
            .filter_map(|pool_id| state.states.get(pool_id).map(|pool| (pool_id, pool)))
            .filter_map(|(pool_id, state)| {
                match simulation::PoolSteps::from_protocol_sim(&pair, steps, inventory, state.as_ref()) {
                    Ok(pool_sim) => Some((pool_id.clone(), pool_sim)),
                    Err(e) => {
                        error!(error = %e, pool.id = %pool_id, pair = %pair, "precompute failed, skipping pool");
                        None
                    }
                }
            });

        pool_sims.extend(precomputes);

        let spot_prices_a_to_b_sorted: Vec<(state::PoolId, f64)> =
            make_sorted_spot_prices(&state, &pair, Direction::AtoB);
        let spot_prices_b_to_a_sorted: Vec<(state::PoolId, f64)> =
            make_sorted_spot_prices(&state, &pair, Direction::BtoA);

        if spot_prices_a_to_b_sorted.is_empty() || spot_prices_b_to_a_sorted.is_empty() {
            trace!(pair= %pair, "No spot prices found");
        } else {
            trace!(
                // min a->b
                min.pool_id = %spot_prices_a_to_b_sorted[0].0,
                min.a_to_b.price = %spot_prices_a_to_b_sorted[0].1,
                min.b_to_a.price = %spot_prices_b_to_a_sorted[0].1,
                // max a->b
                max.pool_id = %spot_prices_a_to_b_sorted[spot_prices_a_to_b_sorted.len() - 1].0,
                max.a_to_b.price = %spot_prices_a_to_b_sorted[spot_prices_a_to_b_sorted.len() - 1].1,
                max.b_to_a.price = %spot_prices_b_to_a_sorted[spot_prices_b_to_a_sorted.len() - 1].1,
                "Computed spot prices for slow chain");
        }

        Self {
            block_height,
            pool_sims,
            sorted_spot_prices: (spot_prices_a_to_b_sorted, spot_prices_b_to_a_sorted),
            pool_metadata: state.metadata.clone(),
            // chain: todo!(),
            // pair: todo!(),
        }
    }
}
