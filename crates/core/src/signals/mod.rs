use serde::{Deserialize, Serialize};
use std::{fmt::Display, sync::Arc};
use tycho_simulation::protocol::models::ProtocolComponent;

use color_eyre::eyre::{self, Context, Ok, OptionExt};

use crate::{
    chain::Chain,
    encoder::{UnsignedTransaction, create_solution},
    spot_prices::SpotPrices,
    state::{self, pair::Pair},
    strategy::Swap,
    trade::Trade,
};

pub use profit::bps_discount;

mod profit;

pub use profit::ExpectedProfit;
pub use profit::RealizedProfit;

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
    pub slow_protocol_component: Option<Arc<ProtocolComponent>>,
    pub slow_pool_id: state::PoolId,
    pub slow_swap_sim: Swap,
    pub slow_height: u64,
    pub fast_chain: Chain,
    pub fast_pair: Pair,
    pub fast_protocol_component: Option<Arc<ProtocolComponent>>,
    pub fast_pool_id: state::PoolId,
    pub fast_swap_sim: Swap,
    pub fast_height: u64,
    pub slow_prices_a_b: SpotPrices,
    pub slow_prices_a_usdc: Option<SpotPrices>,
    pub slow_prices_b_usdc: Option<SpotPrices>,
    pub max_slippage_bps: u64,
    pub congestion_risk_discount_bps: u64,
    pub expected_profit: ExpectedProfit,
}

impl CrossChainSingleHop {
    #[allow(clippy::too_many_arguments)]
    pub fn try_from_simulations(
        slow_chain: &Chain,
        slow_pair: &Pair,
        slow_protocol_component: Arc<ProtocolComponent>,
        slow_id: &state::PoolId,
        slow_height: u64,
        slow_swap_sim: Swap,
        slow_prices_a_b: &SpotPrices,
        slow_prices_a_usdc: &Option<SpotPrices>,
        slow_prices_b_usdc: &Option<SpotPrices>,
        fast_chain: &Chain,
        fast_pair: &Pair,
        fast_protocol_component: Arc<ProtocolComponent>,
        fast_id: &state::PoolId,
        fast_height: u64,
        fast_swap_sim: Swap,
        max_slippage_bps: u64,
        congestion_risk_discount_bps: u64,
    ) -> eyre::Result<Self> {
        if slow_swap_sim.amount_out < fast_swap_sim.amount_in {
            eyre::bail!("Slow chain output is less than fast chain input");
        }

        let expected_profit = ExpectedProfit::try_from_swaps(
            &slow_swap_sim,
            &fast_swap_sim,
            &slow_prices_a_usdc,
            &slow_prices_b_usdc,
            max_slippage_bps,
            congestion_risk_discount_bps,
        )?;

        Ok(Self {
            slow_chain: slow_chain.clone(),
            slow_pair: slow_pair.clone(),
            slow_protocol_component: Some(slow_protocol_component),
            slow_height,
            slow_pool_id: slow_id.clone(),
            slow_swap_sim,
            fast_chain: fast_chain.clone(),
            fast_pair: fast_pair.clone(),
            fast_protocol_component: Some(fast_protocol_component),
            fast_height,
            fast_pool_id: fast_id.clone(),
            fast_swap_sim,
            slow_prices_a_b: slow_prices_a_b.clone(),
            slow_prices_a_usdc: slow_prices_a_usdc.clone(),
            slow_prices_b_usdc: slow_prices_b_usdc.clone(),
            expected_profit,
            max_slippage_bps,
            congestion_risk_discount_bps,
        })
    }

    pub fn try_promote(&self) -> eyre::Result<Trade> {
        let Self {
            slow_chain,
            slow_protocol_component,
            slow_swap_sim,
            fast_chain,
            fast_protocol_component,
            fast_swap_sim,
            ..
        } = self;

        let slow_solution = {
            let slow_component = slow_protocol_component
                .clone()
                .ok_or_eyre("missing protocol component")?
                .as_ref()
                .clone();
            create_solution(slow_component, slow_swap_sim, slow_chain.signer().clone())?
        };

        let fast_solution = {
            let fast_component = fast_protocol_component
                .clone()
                .ok_or_eyre("missing protocol component")?
                .as_ref()
                .clone();
            create_solution(fast_component, fast_swap_sim, fast_chain.signer().clone())?
        };

        let slow_unsigned_tx =
            UnsignedTransaction::try_from_solution(&slow_solution, slow_chain)
                .wrap_err("Failed to create unsigned transaction from slow solution")?;
        let fast_unsigned_tx =
            UnsignedTransaction::try_from_solution(&fast_solution, fast_chain)
                .wrap_err("Failed to create unsigned transaction from fast solution")?;

        // add gas prices and sign transactions
        Ok(Trade::new(self.clone(), slow_unsigned_tx, fast_unsigned_tx))
    }
}

impl Display for CrossChainSingleHop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: get rid of this
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
                Max Slippage bps: {}
            Expected Profit: {}
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
            self.expected_profit
        )
    }
}
