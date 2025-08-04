use std::fmt::Display;

use color_eyre::eyre::{self, Context as _, eyre};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use tracing::debug;
use tycho_common::{models::token::Token, simulation::protocol_sim::ProtocolSim};

use crate::{
    signals::Direction,
    state::{
        PoolId,
        pair::{Pair, PairState},
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Swap {
    pub token_in: Token,
    pub amount_in: BigUint,
    pub token_out: Token,
    pub amount_out: BigUint,
    #[allow(dead_code)]
    pub gas_cost: BigUint,
}

impl Swap {
    pub fn from_protocol_sim(
        amount_in: &BigUint,
        token_in: &Token,
        token_out: &Token,
        protocol_sim: &dyn ProtocolSim,
    ) -> eyre::Result<Self> {
        // TODO: what to do with sim_result.new_state? we dont keep it in Swap because
        // we derive Deserialize (would need to add a Raw with a Swap::try_from_raw())
        let sim_result = protocol_sim
            .get_amount_out(amount_in.clone(), token_in, token_out)
            .wrap_err("simulation failed")?;
        Ok(Self {
            token_in: token_in.clone(),
            amount_in: amount_in.clone(),
            token_out: token_out.clone(),
            amount_out: sim_result.amount,
            gas_cost: sim_result.gas,
        })
    }
}

impl Display for Swap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Swap({:} ({:}) -> {:} ({:}), gas_cost: {:})",
            self.amount_in,
            self.token_in.symbol,
            self.amount_out,
            self.token_out.symbol,
            self.gas_cost
        )
    }
}

// NOTE: This is kind of an order book representation of the amm - the price at different depths
#[derive(Debug, Clone)]
pub struct PoolSteps {
    #[allow(dead_code)]
    pub a_to_b: Vec<Swap>,
    pub b_to_a: Vec<Swap>,
}

impl PoolSteps {
    pub fn from_protocol_sim(
        pair: &Pair,
        steps: usize,
        inventory: &(BigUint, BigUint),
        protocol_sim: &dyn ProtocolSim,
    ) -> eyre::Result<Self> {
        let a_to_b = Self::for_direction(pair, Direction::AtoB, steps, &inventory.0, protocol_sim)
            .wrap_err("failed to simulate a->b swaps")?;
        let b_to_a = Self::for_direction(pair, Direction::BtoA, steps, &inventory.1, protocol_sim)
            .wrap_err("failed to simulate b->a swaps")?;

        Ok(Self { a_to_b, b_to_a })
    }

    fn for_direction(
        pair: &Pair,
        direction: Direction,
        steps: usize,
        inventory: &BigUint,
        protocol_sim: &dyn ProtocolSim,
    ) -> eyre::Result<Vec<Swap>> {
        let mut sims = vec![];

        if steps == 0 {
            return Err(eyre!("steps must be greater than 0. {:} provided", steps));
        }
        // TODO: determine max trade amount based on limits and inventory:
        // min(max_protocol_limit * state.get_limits(), self.max_inventory)
        let step = inventory / steps;
        let (token_in, token_out) = match direction {
            Direction::AtoB => (pair.token_a(), pair.token_b()),
            Direction::BtoA => (pair.token_b(), pair.token_a()),
        };

        for i in 1..=steps {
            let amount_in = &step * i;

            let sim = Swap::from_protocol_sim(
                &amount_in,
                token_in,
                token_out,
                protocol_sim,
            ).wrap_err_with(||
                format!(
                    "swap simulation for {:} -> {:} failed at intermediate step {:} (amount_in {:})\n",
                    pair.token_a().symbol,
                    pair.token_b().symbol,
                    step,
                    amount_in
                ))?;

            // trace!(step = %i, simulation = %sim, "computed simulation");
            sims.push(sim);
        }

        Ok(sims)
    }
}

// NOTE: these are analogous to midprice
pub fn make_sorted_spot_prices(state: &PairState, pair: &Pair) -> Vec<(PoolId, f64)> {
    let mut spots: Vec<(PoolId, f64)> = state
        .states
        .iter()
        .filter_map(|(id, pool)| {
            let spot_price = pool.spot_price(pair.token_a(), pair.token_b());
            match spot_price {
                Ok(price) => Some((id.clone(), price)),
                Err(err) => {
                    debug!(
                        error = %err,
                        pair = %pair,
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
