use std::{collections::HashMap, fmt::Display, ops::Deref as _, path::Display};

use color_eyre::eyre::{self, Context};
use num_bigint::BigUint;
use tracing::{debug, error, trace};
use tycho_common::{models::token::Token, simulation::protocol_sim::ProtocolSim};

use crate::{
    chain::Chain,
    state::{
        self,
        pair::{Pair, PairState},
    },
    strategy::Direction,
};

// TODO: display impl
#[derive(Debug)]
pub struct SimulationResult {
    token_in: Token,
    amount_in: BigUint,
    token_out: Token,
    amount_out: BigUint,
    gas_cost: BigUint,
    new_state: Box<dyn ProtocolSim>,
}

impl SimulationResult {
    pub fn from_protocol_sim(
        amount_in: BigUint,
        token_in: &Token,
        token_out: &Token,
        protocol_sim: &dyn ProtocolSim,
    ) -> eyre::Result<Self> {
        let sim_result = protocol_sim
            .get_amount_out(amount_in.clone(), token_in, token_out)
            .wrap_err("simulation failed")?;
        Ok(Self {
            token_in: token_in.clone(),
            amount_in,
            token_out: token_in.clone(),
            amount_out: sim_result.amount,
            gas_cost: sim_result.gas,
            new_state: sim_result.new_state,
        })
    }
}

// TODO: display impl
#[derive(Debug, Clone)]
pub struct Signal {
    pub slow_pair: Pair,
    pub fast_pair: Pair,
    pub slow_chain: Chain,
    pub fast_chain: Chain,
    pub path: Direction,
    pub slow_chain_amount_out: BigUint,
    pub fast_chain_amount_out: BigUint,
    pub profit_percentage: f64,
    pub optimal_amount_in: BigUint,
    pub expected_profit: BigUint,
}

impl Signal {
    // TODO: from_simulation_result(sim_result) -> Self
    // TODO: cmp(other_signal) -> Ordering
}

impl Display for Signal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!("Implement display for Signal")
    }
}

pub(crate) fn with_slippage_tolerance(amount: &BigUint, slippage_tolerance: f64) -> BigUint {
    let slippage_bps = (slippage_tolerance * 10000.0) as u64;
    let risk_multiplier = BigUint::from(10000u64 - slippage_bps);
    (amount * risk_multiplier) / BigUint::from(10000u64)
}

pub(crate) fn with_risk_factor(amount: &BigUint, risk_factor_bps: u64) -> BigUint {
    let risk_multiplier = BigUint::from(10000u64 - risk_factor_bps);
    (amount * risk_multiplier) / BigUint::from(10000u64)
}

// TODO: fix these tests
#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::Chain;
    use std::{
        collections::{HashMap, HashSet},
        str::FromStr as _,
        sync::Arc,
    };
    use tycho_common::simulation::protocol_sim::ProtocolSim;

    // Helper function to create UniswapV2State for testing
    fn create_uniswap_v2_state_with_liquidity(
        reserve_0: &str,
        reserve_1: &str,
    ) -> Arc<dyn ProtocolSim> {
        use std::str::FromStr;
        use tycho_simulation::evm::protocol::uniswap_v2::state::UniswapV2State;

        let reserve_0_u256 = alloy::primitives::U256::from_str(reserve_0).unwrap();
        let reserve_1_u256 = alloy::primitives::U256::from_str(reserve_1).unwrap();

        Arc::new(UniswapV2State::new(reserve_0_u256, reserve_1_u256))
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

    #[test]
    fn compute_arb_sanity_check() {
        let strategy = Arc::new(CrossChainSingleHop {
            slow_pair: Pair::new(make_mainnet_usdc(), make_mainnet_weth()),
            fast_pair: Pair::new(make_base_usdc(), make_base_weth()),
            min_profit_threshold: 0.5, // 0.5%
            available_inventory: BigUint::from(1000u64),
            binary_search_steps: 5,
            max_slippage_bps: 0.0025,         // 0.25%
            congestion_risk_discount_bps: 25, // 0.25%
        });

        // Create slow state (favors token B)
        let slow_chain = Chain::eth_mainnet();
        let slow_state = PairState {
            states: HashMap::from([(
                state::Id::from("0x123"),
                create_uniswap_v2_state_with_liquidity("950000", "1000000"),
            )]),
            block_number: 100,
            modified_pools: Arc::new(HashSet::from([state::Id::from("0x123")])),
            unmodified_pools: Arc::new(HashSet::new()),
            metadata: HashMap::new(),
        };

        // Create fast state (favors token A)
        let fast_chain = Chain::base_mainnet();
        let fast_state = PairState {
            block_number: 200,
            states: HashMap::from([(
                state::Id::from("0x456"),
                create_uniswap_v2_state_with_liquidity("1000000", "950000"),
            )]),
            modified_pools: Arc::new(HashSet::from([state::Id::from("0x456")])),
            unmodified_pools: Arc::new(HashSet::new()),
            metadata: HashMap::new(),
        };

        // Test precompute
        let precompute = strategy.precompute(&slow_state, &slow_chain);
        assert!(!precompute.calculations.is_empty());
        assert_eq!(precompute.calculations.len(), 10); // 5 steps × 2 paths
        assert_eq!(precompute.chain, slow_chain);

        // Verify precompute calculations
        let first_calc = &precompute.calculations[0];
        assert_eq!(first_calc.amount_in, BigUint::from(200u64)); // 1000 / 5 steps
        assert!(matches!(first_calc.path, Direction::AtoB)); // First path should be AtoB

        let second_calc = &precompute.calculations[1];
        assert_eq!(second_calc.amount_in, BigUint::from(200u64));
        assert!(matches!(second_calc.path, Direction::BtoA)); // Second path should be BtoA

        // compute_arb
        let signal = strategy
            .generate_signal(&precompute, &fast_state, &fast_chain)
            .unwrap();

        // With given reserves and slippage, profit should be in a specific range
        assert!(signal.profit_percentage >= 0.5 && signal.profit_percentage <= 10.0);
        assert!(signal.expected_profit > BigUint::from(0u64));

        // Verify the optimal path and chain assignments
        assert!(matches!(signal.path, Direction::AtoB)); // The profitable direction with these reserves
        assert_eq!(
            signal.slow_chain.chain_id(),
            Chain::eth_mainnet().chain_id()
        );

        assert_eq!(
            signal.fast_chain.chain_id(),
            Chain::base_mainnet().chain_id()
        );

        // Verify token assignments
        // assert_eq!(signal.asset_a.symbol, "USDC");
        // assert_eq!(signal.asset_b.symbol, "WETH");
    }
}
