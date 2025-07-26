use num_traits::CheckedSub;
use std::fmt::Display;

use color_eyre::eyre::{self, ContextCompat};
use num_bigint::BigUint;

use crate::{
    chain::Chain,
    state::{self, pair::Pair},
    strategy::Swap,
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
    pub surplus: (BigUint, BigUint),
    pub expected_profit: (BigUint, BigUint),
    // tx parameters
    pub slow_id: state::PoolId,
    pub slow_sim: Swap,
    pub fast_id: state::PoolId,
    pub fast_sim: Swap,
}

impl CrossChainSingleHop {
    // TODO: should this be fallible?
    // -> failing to construct a signal should be part of the search process
    //
    // constructing a `SimulationRresult` is faillible becasuse it propagates the tycho error
    // this is faillble because it propagates biguint errors - suplus, slippage and expected profit must be greater than zero
    pub fn try_from_simulations(
        slow_chain: &Chain,
        slow_pair: &Pair,
        slow_id: &state::PoolId,
        slow_height: u64,
        slow_sim: Swap,
        fast_chain: &Chain,
        fast_pair: &Pair,
        fast_id: &state::PoolId,
        fast_height: u64,
        fast_sim: Swap,
        max_slippage_bps: u64,
        congestion_risk_discount_bps: u64,
    ) -> eyre::Result<Self> {
        if slow_sim.amount_out < fast_sim.amount_in {
            eyre::bail!("Slow chain output is less than fast chain input");
        }

        // TODO: handle overflow calculations
        let (surplus_a, surplus_b) = calculate_surplus(&slow_sim, &fast_sim)?;

        // TODO: compound two separate congestion risks, one for each side
        let expected_profits = calculate_expected_profits(
            &slow_sim,
            &fast_sim,
            max_slippage_bps,
            congestion_risk_discount_bps,
        )?;

        // TODO: save max slippage for each side?

        Ok(Self {
            slow_chain: slow_chain.clone(),
            slow_pair: slow_pair.clone(),
            slow_height,
            slow_id: slow_id.clone(),
            slow_sim,
            fast_chain: fast_chain.clone(),
            fast_pair: fast_pair.clone(),
            fast_height,
            fast_id: fast_id.clone(),
            fast_sim,
            surplus: (surplus_a, surplus_b),
            expected_profit: expected_profits,
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
            Expected Profit: {} ({}) {} ({})
                Surplus: {} ({}) {} ({})
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
            self.expected_profit.0,
            self.slow_pair.token_a().symbol,
            self.expected_profit.1,
            self.slow_pair.token_b().symbol,
            self.surplus.0,
            self.slow_pair.token_a().symbol,
            self.surplus.1,
            self.slow_pair.token_b().symbol,
        )
    }
}

pub(crate) fn bps_discount(amount: &BigUint, slippage_bps: u64) -> BigUint {
    let slippage_multiplier = BigUint::from(10000u64 - slippage_bps);
    (amount * slippage_multiplier) / BigUint::from(10000u64)
}

pub fn calculate_surplus(slow_sim: &Swap, fast_sim: &Swap) -> eyre::Result<(BigUint, BigUint)> {
    let surplus_a = fast_sim
        .amount_out
        .checked_sub(&slow_sim.amount_in)
        .wrap_err("surplus of token a cannot be negative")?;
    let surplus_b = slow_sim
        .amount_out
        .checked_sub(&fast_sim.amount_in)
        .wrap_err("surplus of token b cannot be negative")?;
    Ok((surplus_a, surplus_b))
}

pub fn calculate_expected_profits(
    slow_sim: &Swap,
    fast_sim: &Swap,
    max_slippage_bps: u64,
    congestion_risk_discount_bps: u64,
) -> eyre::Result<(BigUint, BigUint)> {
    let min_slow_amount_out = bps_discount(&slow_sim.amount_out, max_slippage_bps);
    let min_fast_amount_out = bps_discount(&fast_sim.amount_out, max_slippage_bps);

    let min_surplus_a = min_fast_amount_out
        .checked_sub(&slow_sim.amount_in)
        .wrap_err("min surplus of token a cannot be negative")?;
    let min_surplus_b = min_slow_amount_out
        .checked_sub(&fast_sim.amount_in)
        .wrap_err("min surplus of token b cannot be negative")?;

    Ok((
        bps_discount(&min_surplus_a, congestion_risk_discount_bps),
        bps_discount(&min_surplus_b, congestion_risk_discount_bps),
    ))
}
