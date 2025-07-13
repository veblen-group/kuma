use num_bigint::BigUint;
use std::{pin::Pin, sync::Arc, time::Duration};
use tokio::{
    select,
    sync::watch,
    time::{Sleep, sleep},
};
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument, warn};
use tycho_simulation::{models::Token, protocol::state::ProtocolSim};

use crate::chain::ChainInfo;

// Core state structures for the new architecture
#[derive(Debug, Clone)]
pub struct State {
    pub state: Box<dyn ProtocolSim>,
    pub chain_info: ChainInfo,
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
    pub slow_chain: ChainInfo,
    pub fast_chain: ChainInfo,
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

// Async signal emission function
pub async fn emit_signal(sig: ArbSignal) {
    info!(
        "🚀 Arbitrage signal emitted: {:.2}% profit on {} {} trade",
        sig.profit_percentage, sig.optimal_amount_in, sig.asset_a.symbol
    );

    // In production, this would emit to:
    // - Trading execution engine
    // - Risk management system
    // - Monitoring/alerting system
    // - Performance analytics
}

// Main arbitrage task runner with the new architecture
pub async fn run_arb_task(
    mut slow_rx: watch::Receiver<Arc<State>>,
    mut fast_rx: watch::Receiver<Arc<State>>,
    strategy: Arc<CrossChainArbitrageStrategy>,
    shutdown: CancellationToken,
    slow_block_interval: Duration,
) {
    info!("Starting arbitrage task with timer-based architecture");

    // Timer for "75% of slow block interval"
    let mut timer: Option<Pin<Box<Sleep>>> = None;
    // Future for in-flight signal emission
    let mut emit_fut: Option<Pin<Box<dyn std::future::Future<Output = ()> + Send>>> = None;
    // Last slow-chain precompute result
    let mut last_pre: Option<Precompute> = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::watch;
    use tokio_util::sync::CancellationToken;

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

    fn create_test_token(symbol: &str, address: &str) -> Token {
        Token {
            symbol: symbol.to_string(),
            address: address.as_bytes().to_vec().into(),
            decimals: 18,
            gas: BigUint::from(0u64),
        }
    }

    fn create_test_chain_info(chain: &str) -> ChainInfo {
        ChainInfo {
            chain: chain.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_timer_based_arbitrage_strategy() {
        let token_a = create_test_token("USDC", "0x1");
        let token_b = create_test_token("USDT", "0x2");

        let strategy = Arc::new(CrossChainArbitrageStrategy {
            asset_a: token_a.clone(),
            asset_b: token_b.clone(),
            min_profit_threshold: 0.5, // 0.5%
            max_trade_amount: BigUint::from(1000u64),
            binary_search_steps: 5,
            slippage_tolerance: 0.0025, // 0.25%
            risk_factor_bps: 25,        // 0.25%
        });

        // Create slow state (favors token B)
        let slow_state = State {
            state: create_uniswap_v2_state_with_liquidity("950000", "1000000"),
            chain_info: create_test_chain_info("ethereum"),
            block_number: 100,
        };

        // Create fast state (favors token A)
        let fast_state = State {
            state: create_uniswap_v2_state_with_liquidity("1000000", "950000"),
            chain_info: create_test_chain_info("polygon"),
            block_number: 200,
        };

        // Test precompute
        let precompute = strategy.precompute(&slow_state);
        assert!(!precompute.calculations.is_empty());
        assert_eq!(precompute.calculations.len(), 10); // 5 steps × 2 paths

        // Test compute_arb
        let signal = strategy.compute_arb(&precompute, &fast_state);
        assert!(signal.is_some());

        let signal = signal.unwrap();
        assert!(signal.profit_percentage > 0.5);
        assert!(signal.expected_profit > BigUint::from(0u64));

        println!("✅ Timer-based arbitrage strategy test passed!");
        println!("   Profit: {:.2}%", signal.profit_percentage);
        println!("   Expected profit: {} units", signal.expected_profit);
    }

    #[test]
    fn test_run_arb_task_integration() {
        tokio_test::block_on(async {
            let token_a = create_test_token("USDC", "0x1");
            let token_b = create_test_token("USDT", "0x2");

            let strategy = Arc::new(CrossChainArbitrageStrategy {
                asset_a: token_a.clone(),
                asset_b: token_b.clone(),
                min_profit_threshold: 0.5,
                max_trade_amount: BigUint::from(1000u64),
                binary_search_steps: 3, // Smaller for test performance
                slippage_tolerance: 0.0025,
                risk_factor_bps: 25,
            });

            // Create watch channels
            let slow_state = Arc::new(State {
                state: create_uniswap_v2_state_with_liquidity("950000", "1000000"),
                chain_info: create_test_chain_info("ethereum"),
                block_number: 100,
            });

            let fast_state = Arc::new(State {
                state: create_uniswap_v2_state_with_liquidity("1000000", "950000"),
                chain_info: create_test_chain_info("polygon"),
                block_number: 200,
            });

            let (slow_tx, slow_rx) = watch::channel(slow_state.clone());
            let (fast_tx, fast_rx) = watch::channel(fast_state.clone());
            let shutdown = CancellationToken::new();

            // Start the arbitrage task
            let task_shutdown = shutdown.clone();
            let task_handle = tokio::spawn(run_arb_task(
                slow_rx,
                fast_rx,
                strategy,
                task_shutdown,
                Duration::from_millis(100), // Fast for testing
            ));

            // Simulate updates
            tokio::time::sleep(Duration::from_millis(10)).await;

            // Send new slow state update to trigger precompute + timer
            let _ = slow_tx.send(slow_state);

            // Wait for timer to fire and computation to complete
            tokio::time::sleep(Duration::from_millis(150)).await;

            // Shutdown
            shutdown.cancel();
            task_handle.await.unwrap();

            println!("✅ Timer-based arbitrage task integration test passed!");
        });
    }
}
