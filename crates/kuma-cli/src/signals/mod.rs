use num_bigint::BigUint;
use tycho_common::{models::token::Token, simulation::protocol_sim::ProtocolSim};

use crate::chain::Chain;

// Core state structures for the new architecture
#[derive(Debug, Clone)]
pub struct State {
    pub state: Box<dyn ProtocolSim>,
    pub chain_info: Chain,
    pub block_number: u64,
}

#[derive(Debug, Clone)]
pub struct Precompute {
    pub calculations: Vec<SlowChainCalculation>,
    pub slow_state: State,
}

#[derive(Debug, Clone)]
pub struct SlowChainCalculation {
    pub path: Direction,
    pub amount_in: BigUint,
    pub amount_out: BigUint,
    pub input_token: Token,
    pub output_token: Token,
}

#[derive(Debug, Clone)]
pub enum Direction {
    AtoB, // A->B->A
    BtoA, // B->A->B
}

#[derive(Debug, Clone)]
pub struct ArbSignal {
    pub asset_a: Token,
    pub asset_b: Token,
    pub slow_chain: Chain,
    pub fast_chain: Chain,
    pub path: Direction,
    pub slow_chain_amount_out: BigUint,
    pub fast_chain_amount_out: BigUint,
    pub profit_percentage: f64,
    pub optimal_amount_in: BigUint,
    pub expected_profit: BigUint,
}

// Implementation of the arbitrage strategy
pub struct CrossChainArbitrageStrategy {
    pub asset_a: Token,
    pub asset_b: Token,
    pub min_profit_threshold: f64,
    // TODO: use limits
    pub max_trade_amount: BigUint,
    pub binary_search_steps: usize,
    pub slippage_tolerance: f64,
    pub risk_factor_bps: u64,
}

impl CrossChainArbitrageStrategy {
    fn precompute(&self, slow_state: &State) -> Precompute {
        let mut calculations = Vec::new();

        // Create amount input ranges for binary search
        // TODO: determine max trade amount based on limits and inventory. min(self.max_protocol_limit * state.get_limits(), self.max_inventory)
        let step_size = &self.max_trade_amount / BigUint::from(self.binary_search_steps as u64);

        // TODO: parrelize this loop
        for i in 1..=self.binary_search_steps {
            let amount_in = &step_size * BigUint::from(i as u64);

            // Path A->B->A: Start with asset A, swap to B on slow chain
            if let Ok(amount_out_a_to_b) =
                self.simulate_swap(&slow_state.state, &self.asset_a, &self.asset_b, &amount_in)
            {
                calculations.push(SlowChainCalculation {
                    path: Direction::AtoB,
                    amount_in: amount_in.clone(),
                    amount_out: amount_out_a_to_b,
                    input_token: self.asset_a.clone(),
                    output_token: self.asset_b.clone(),
                });
            }

            // Path B->A->B: Start with asset B, swap to A on slow chain
            if let Ok(amount_out_b_to_a) =
                self.simulate_swap(&slow_state.state, &self.asset_b, &self.asset_a, &amount_in)
            {
                calculations.push(SlowChainCalculation {
                    path: Direction::BtoA,
                    amount_in: amount_in.clone(),
                    amount_out: amount_out_b_to_a,
                    input_token: self.asset_b.clone(),
                    output_token: self.asset_a.clone(),
                });
            }
        }

        Precompute {
            calculations,
            slow_state: slow_state.clone(),
        }
    }

    fn compute_arb(&self, precompute: &Precompute, fast_state: &State) -> Option<ArbSignal> {
        let mut best_signal: Option<ArbSignal> = None;
        let mut best_profit = BigUint::from(0u64);

        // TODO: parrelize this loop
        for slow_calc in &precompute.calculations {
            // Complete the arbitrage path based on the slow chain calculation

            // Use the amount_out from slow chain as amount_in for fast chain
            if let Ok(fast_amount_out) = self.simulate_swap(
                &fast_state.state,
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

                    if profit_percentage >= self.min_profit_threshold && profit > best_profit {
                        best_profit = profit.clone();
                        best_signal = Some(ArbSignal {
                            asset_a: self.asset_a.clone(),
                            asset_b: self.asset_b.clone(),
                            slow_chain: precompute.slow_state.chain_info.clone(),
                            fast_chain: fast_state.chain_info.clone(),
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
        state: &Box<dyn ProtocolSim>,
        token_in: &Token,
        token_out: &Token,
        amount_in: &BigUint,
    ) -> Result<BigUint, String> {
        // Get the swap result using Tycho's simulation
        let swap_result = state
            .get_amount_out(amount_in.clone(), token_in, token_out)
            .map_err(|e| format!("Swap simulation failed: {:?}", e))?;

        let mut min_amount_out =
            with_slippage_tolerance(&swap_result.amount, self.slippage_tolerance);
        min_amount_out = with_risk_factor(&swap_result.amount, self.risk_factor_bps);

        Ok(min_amount_out)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::Chain;
    use std::{str::FromStr as _, sync::Arc};
    use tycho_common::simulation::protocol_sim::ProtocolSim;

    // Helper function to create UniswapV2State for testing
    fn create_uniswap_v2_state_with_liquidity(
        reserve_0: &str,
        reserve_1: &str,
    ) -> Box<dyn ProtocolSim> {
        use std::str::FromStr;
        use tycho_simulation::evm::protocol::uniswap_v2::state::UniswapV2State;

        let reserve_0_u256 = alloy::primitives::U256::from_str(reserve_0).unwrap();
        let reserve_1_u256 = alloy::primitives::U256::from_str(reserve_1).unwrap();

        Box::new(UniswapV2State::new(reserve_0_u256, reserve_1_u256))
    }

    fn make_weth() -> Token {
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
    fn make_usdc() -> Token {
        Token::new(
            &tycho_common::Bytes::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            "USDC",
            6,
            1000,
            &[Some(1000u64)],
            tycho_common::models::Chain::Ethereum,
            100,
        )
    }

    #[test]
    fn compute_arb_sanity_check() {
        let usdc = make_usdc();
        let weth = make_weth();

        let strategy = Arc::new(CrossChainArbitrageStrategy {
            asset_a: usdc.clone(),
            asset_b: weth.clone(),
            min_profit_threshold: 0.5, // 0.5%
            max_trade_amount: BigUint::from(1000u64),
            binary_search_steps: 5,
            slippage_tolerance: 0.0025, // 0.25%
            risk_factor_bps: 25,        // 0.25%
        });

        // Create slow state (favors token B)
        let slow_state = State {
            state: create_uniswap_v2_state_with_liquidity("950000", "1000000"),
            chain_info: Chain::eth_mainnet(),
            block_number: 100,
        };

        // Create fast state (favors token A)
        let fast_state = State {
            state: create_uniswap_v2_state_with_liquidity("1000000", "950000"),
            chain_info: Chain::eth_mainnet(),
            block_number: 200,
        };

        // Test precompute
        let precompute = strategy.precompute(&slow_state);
        assert!(!precompute.calculations.is_empty());
        assert_eq!(precompute.calculations.len(), 10); // 5 steps × 2 paths

        // Verify precompute calculations
        let first_calc = &precompute.calculations[0];
        assert_eq!(first_calc.amount_in, BigUint::from(200u64)); // 1000 / 5 steps
        assert!(matches!(first_calc.path, Direction::AtoB)); // First path should be AtoB

        let second_calc = &precompute.calculations[1];
        assert_eq!(second_calc.amount_in, BigUint::from(200u64));
        assert!(matches!(second_calc.path, Direction::BtoA)); // Second path should be BtoA

        // Test compute_arb
        let signal = strategy.compute_arb(&precompute, &fast_state);
        assert!(signal.is_some());

        let signal = signal.unwrap();
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
            Chain::eth_mainnet().chain_id()
        );

        // Verify token assignments
        assert_eq!(signal.asset_a.symbol, "USDC");
        assert_eq!(signal.asset_b.symbol, "WETH");
    }
}
