use std::fmt::Display;

use color_eyre::eyre::{self, Context};
use num_bigint::BigUint;
use tracing::instrument;
use tycho_common::{models::token::Token, simulation::protocol_sim::ProtocolSim};

use crate::{
    chain::Chain,
    state::{self, pair::Pair},
};

// TODO: rename to buy/sell? need to clarify the direction
#[derive(Debug, Clone)]
pub enum Direction {
    AtoB,
    BtoA,
}

impl Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::AtoB => write!(f, "A to B"),
            Direction::BtoA => write!(f, "B to A"),
        }
    }
}

// TODO: display impl
// TODO: maybe simulation::Output?
#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub token_in: Token,
    pub amount_in: BigUint,
    pub token_out: Token,
    pub amount_out: BigUint,
    #[allow(dead_code)]
    pub gas_cost: BigUint,
    #[allow(dead_code)]
    pub new_state: Box<dyn ProtocolSim>,
}

impl SimulationResult {
    pub fn from_protocol_sim(
        amount_in: &BigUint,
        token_in: &Token,
        token_out: &Token,
        protocol_sim: &dyn ProtocolSim,
    ) -> eyre::Result<Self> {
        let sim_result = protocol_sim
            .get_amount_out(amount_in.clone(), token_in, token_out)
            .wrap_err("simulation failed")?;
        Ok(Self {
            token_in: token_in.clone(),
            amount_in: amount_in.clone(),
            token_out: token_in.clone(),
            amount_out: sim_result.amount,
            gas_cost: sim_result.gas,
            new_state: sim_result.new_state,
        })
    }
}

// TODO: display impl
#[derive(Debug, Clone)]
pub struct CrossChainSingleHop {
    // TODO: for each of slow and fast chains
    // - chain_metadata: (chain, pair)
    // - block: (height, inventory)
    // - optimal_swap state::Id, simulation_result
    // - slippage config, risk parameters, expect profit
    // TODO: this should all be per chain
    // static metadata
    slow_chain: Chain,
    slow_pair: Pair,
    slow_height: u64,
    fast_chain: Chain,
    fast_pair: Pair,
    fast_height: u64,
    max_slippage_bps: u64,
    // TODO: use this in display impl
    #[allow(dead_code)]
    congestion_risk_discount_bps: u64,
    surplus: BigUint,
    // tx parameters
    pub slow_id: state::Id,
    pub slow_sim: SimulationResult,
    pub fast_id: state::Id,
    pub fast_sim: SimulationResult,
    pub expected_profit: BigUint,
}

impl CrossChainSingleHop {
    #[instrument]
    // TODO: should this be fallible?
    // -> failing to construct a signal should be part of the search process
    //
    // constructing a `SimulationRresult` is faillible becasuse it propagates the tycho error
    pub fn try_from_simulations(
        slow_chain: Chain,
        slow_pair: Pair,
        slow_id: state::Id,
        slow_height: u64,
        slow_sim: SimulationResult,
        fast_chain: Chain,
        fast_pair: Pair,
        fast_id: state::Id,
        fast_height: u64,
        fast_sim: SimulationResult,
        max_slippage_bps: u64,
        congestion_risk_discount_bps: u64,
    ) -> eyre::Result<Self> {
        if slow_sim.amount_out < fast_sim.amount_in {
            eyre::bail!("Slow chain output is less than fast chain input");
        }

        // TODO: calculate surplus expected profit
        let surplus =
            &slow_sim.amount_out - &slow_sim.amount_in + &fast_sim.amount_out - &fast_sim.amount_in;

        let min_slow_amount =
            bps_discount(&slow_sim.amount_out, max_slippage_bps) - &slow_sim.amount_in;

        let min_fast_amount =
            bps_discount(&fast_sim.amount_out, max_slippage_bps) - &fast_sim.amount_in;

        // TODO: compound two separate congestion risks, one for each side
        let expected_profit = bps_discount(
            &(&min_slow_amount + &min_fast_amount),
            congestion_risk_discount_bps,
        );

        // TODO: save max slippage for each side?

        Ok(Self {
            slow_chain,
            slow_pair,
            slow_height,
            slow_id,
            slow_sim,
            fast_chain,
            fast_pair,
            fast_height,
            fast_id,
            fast_sim,
            surplus,
            expected_profit,
            max_slippage_bps,
            congestion_risk_discount_bps,
        })
    }
}

impl Display for CrossChainSingleHop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let max_slippage_slow = &self.slow_sim.amount_out
            - bps_discount(&self.slow_sim.amount_out, self.max_slippage_bps);
        let max_slippage_fast = &self.fast_sim.amount_out
            - bps_discount(&self.fast_sim.amount_out, self.max_slippage_bps);

        write!(
            f,
            "🐌 Slow Chain:
                Chain: {}
                Pair: {}
                Height: {}
                ID: {}
                Amount In: {}
                Amount Out: {}
                Max Slippage: {}
            🐇 Fast Chain:
                Chain: {}
                Pair: {}
                Height: {}
                ID: {}
                Amount In: {}
                Amount Out: {}
                Max Slippage: {}
            Expected Profit: {}
                Surplus: {}
            ",
            self.slow_chain,
            self.slow_pair,
            self.slow_height,
            self.slow_id,
            self.slow_sim.amount_in,
            self.slow_sim.amount_out,
            max_slippage_slow,
            self.fast_chain,
            self.fast_pair,
            self.fast_height,
            self.fast_id,
            self.fast_sim.amount_in,
            self.fast_sim.amount_out,
            max_slippage_fast,
            self.expected_profit,
            self.surplus
        )
    }
}

pub(crate) fn bps_discount(amount: &BigUint, slippage_bps: u64) -> BigUint {
    let slippage_multiplier = BigUint::from(10000u64 - slippage_bps);
    (amount * slippage_multiplier) / BigUint::from(10000u64)
}
