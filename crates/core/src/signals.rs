use num_traits::CheckedSub;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, sync::Arc};
use tycho_simulation::protocol::models::ProtocolComponent;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossChainSingleHop {
    pub slow_chain: Chain,
    pub slow_pair: Pair,
    #[serde(skip)]
    pub slow_protocol_component: Option<Arc<ProtocolComponent>>,
    pub slow_pool_id: state::PoolId,
    pub slow_swap_sim: Swap,
    pub slow_height: u64,
    pub fast_chain: Chain,
    pub fast_pair: Pair,
    #[serde(skip)]
    pub fast_protocol_component: Option<Arc<ProtocolComponent>>,
    pub fast_pool_id: state::PoolId,
    pub fast_swap_sim: Swap,
    pub fast_height: u64,
    pub max_slippage_bps: u64,
    pub congestion_risk_discount_bps: u64,
    pub surplus: (BigUint, BigUint),
    pub expected_profit: (BigUint, BigUint),
}

impl CrossChainSingleHop {
    pub fn try_from_simulations(
        slow_chain: &Chain,
        slow_pair: &Pair,
        slow_protocol_component: Arc<ProtocolComponent>,
        slow_id: &state::PoolId,
        slow_height: u64,
        slow_sim: Swap,
        fast_chain: &Chain,
        fast_pair: &Pair,
        fast_protocol_component: Arc<ProtocolComponent>,
        fast_id: &state::PoolId,
        fast_height: u64,
        fast_sim: Swap,
        max_slippage_bps: u64,
        congestion_risk_discount_bps: u64,
    ) -> eyre::Result<Self> {
        if slow_sim.amount_out < fast_sim.amount_in {
            eyre::bail!("Slow chain output is less than fast chain input");
        }

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
            slow_protocol_component: Some(slow_protocol_component),
            slow_height,
            slow_pool_id: slow_id.clone(),
            slow_swap_sim: slow_sim,
            fast_chain: fast_chain.clone(),
            fast_pair: fast_pair.clone(),
            fast_protocol_component: Some(fast_protocol_component),
            fast_height,
            fast_pool_id: fast_id.clone(),
            fast_swap_sim: fast_sim,
            surplus: (surplus_a, surplus_b),
            expected_profit: expected_profits,
            max_slippage_bps,
            congestion_risk_discount_bps,
        })
    }
}

impl Display for CrossChainSingleHop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let max_slippage_slow = &self.slow_swap_sim.amount_out
            - bps_discount(&self.slow_swap_sim.amount_out, self.max_slippage_bps);
        let max_slippage_fast = &self.fast_swap_sim.amount_out
            - bps_discount(&self.fast_swap_sim.amount_out, self.max_slippage_bps);

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
            self.slow_pool_id,
            self.slow_swap_sim.amount_in,
            self.slow_swap_sim.amount_out,
            max_slippage_slow,
            self.fast_chain,
            self.fast_pair,
            self.fast_height,
            self.fast_pool_id,
            self.fast_swap_sim.amount_in,
            self.fast_swap_sim.amount_out,
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
        .wrap_err_with(|| {
            format!(
                "surplus of token a cannot be negative: fast.amount_out - slow.amount_in = {} - {} ",
                fast_sim.amount_out, slow_sim.amount_in
            )
        })?;
    let surplus_b = slow_sim
        .amount_out
        .checked_sub(&fast_sim.amount_in)
        .wrap_err_with(|| {
            format!(
                "surplus of token b cannot be negative: slow.amount_out={} - fast.amount_in={} ",
                slow_sim.amount_out, fast_sim.amount_in
            )
        })?;
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
