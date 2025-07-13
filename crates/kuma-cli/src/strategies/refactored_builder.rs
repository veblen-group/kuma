use num_bigint::BigUint;
use std::{pin::Pin, sync::Arc, time::Duration};
use tokio::{
    select,
    sync::watch,
    task,
    time::{Sleep, sleep},
};
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument, warn};
use tycho_simulation::{models::Token, protocol::state::ProtocolSim};

use crate::chain::ChainInfo;

// Core state structures for the new architecture
#[derive(Debug, Clone)]
pub struct SlowState {
    pub state: Box<dyn ProtocolSim>,
    pub chain_info: ChainInfo,
    pub block_number: u64,
}

#[derive(Debug, Clone)]
pub struct FastState {
    pub state: Box<dyn ProtocolSim>,
    pub chain_info: ChainInfo,
    pub block_number: u64,
}

#[derive(Debug, Clone)]
pub struct SlowPrecompute {
    pub calculations: Vec<SlowChainCalculation>,
    pub slow_state: SlowState,
}

#[derive(Debug, Clone)]
pub struct SlowChainCalculation {
    pub path: ArbitragePath,
    pub amount_in: BigUint,
    pub amount_out: BigUint,
    pub input_token: Token,
    pub output_token: Token,
}

#[derive(Debug, Clone)]
pub enum ArbitragePath {
    AtoB, // A->B->A
    BtoA, // B->A->B
}

#[derive(Debug, Clone)]
pub struct ArbSignal {
    pub asset_a: Token,
    pub asset_b: Token,
    pub slow_chain: ChainInfo,
    pub fast_chain: ChainInfo,
    pub path: ArbitragePath,
    pub slow_chain_amount_out: BigUint,
    pub fast_chain_amount_out: BigUint,
    pub profit_percentage: f64,
    pub optimal_amount_in: BigUint,
    pub expected_profit: BigUint,
}

// The main arbitrage strategy trait
pub trait ArbStrategy: Send + Sync {
    fn precompute(&self, slow_state: &SlowState) -> SlowPrecompute;
    fn compute_arb(&self, precompute: &SlowPrecompute, fast_state: &FastState)
    -> Option<ArbSignal>;
}

// Implementation of the arbitrage strategy
pub struct CrossChainArbitrageStrategy {
    pub asset_a: Token,
    pub asset_b: Token,
    pub min_profit_threshold: f64,
    pub max_trade_amount: BigUint,
    pub binary_search_steps: usize,
    pub slippage_tolerance: f64,
    pub risk_factor_bps: u64,
}

impl ArbStrategy for CrossChainArbitrageStrategy {
    fn precompute(&self, slow_state: &SlowState) -> SlowPrecompute {
        let mut calculations = Vec::new();

        // Create amount input ranges for binary search
        let step_size = &self.max_trade_amount / BigUint::from(self.binary_search_steps as u64);

        for i in 1..=self.binary_search_steps {
            let amount_in = &step_size * BigUint::from(i as u64);

            // Path A->B->A: Start with asset A, swap to B on slow chain
            if let Ok(amount_out_a_to_b) =
                self.simulate_swap(&slow_state.state, &self.asset_a, &self.asset_b, &amount_in)
            {
                calculations.push(SlowChainCalculation {
                    path: ArbitragePath::AtoB,
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
                    path: ArbitragePath::BtoA,
                    amount_in: amount_in.clone(),
                    amount_out: amount_out_b_to_a,
                    input_token: self.asset_b.clone(),
                    output_token: self.asset_a.clone(),
                });
            }
        }

        SlowPrecompute {
            calculations,
            slow_state: slow_state.clone(),
        }
    }

    fn compute_arb(
        &self,
        precompute: &SlowPrecompute,
        fast_state: &FastState,
    ) -> Option<ArbSignal> {
        let mut best_signal: Option<ArbSignal> = None;
        let mut best_profit = BigUint::from(0u64);

        for slow_calc in &precompute.calculations {
            // Complete the arbitrage path based on the slow chain calculation
            let (fast_input_token, fast_output_token) = match slow_calc.path {
                ArbitragePath::AtoB => {
                    // A->B->A: slow chain produced B, fast chain should convert B back to A
                    (&slow_calc.output_token, &slow_calc.input_token)
                }
                ArbitragePath::BtoA => {
                    // B->A->B: slow chain produced A, fast chain should convert A back to B
                    (&slow_calc.output_token, &slow_calc.input_token)
                }
            };

            // Use the amount_out from slow chain as amount_in for fast chain
            if let Ok(fast_amount_out) = self.simulate_swap(
                &fast_state.state,
                fast_input_token,
                fast_output_token,
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
}

impl CrossChainArbitrageStrategy {
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

        // Apply slippage tolerance and risk factor to get minimum amount out
        let slippage_bps = (self.slippage_tolerance * 10000.0) as u64;
        let total_risk_bps = slippage_bps + self.risk_factor_bps;
        let risk_multiplier = BigUint::from(10000u64 - total_risk_bps);
        let min_amount_out = (&swap_result.amount * &risk_multiplier) / BigUint::from(10000u64);

        Ok(min_amount_out)
    }
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
    mut slow_rx: watch::Receiver<Arc<SlowState>>,
    mut fast_rx: watch::Receiver<Arc<FastState>>,
    strategy: Arc<dyn ArbStrategy>,
    shutdown: CancellationToken,
    slow_block_interval: Duration,
) {
    info!("Starting arbitrage task with timer-based architecture");

    // Timer for "75% of slow block interval"
    let mut timer: Option<Pin<Box<Sleep>>> = None;
    // Future for in-flight signal emission
    let mut emit_fut: Option<Pin<Box<dyn std::future::Future<Output = ()> + Send>>> = None;
    // Last slow-chain precompute result
    let mut last_pre: Option<SlowPrecompute> = None;

    loop {
        match (&mut timer, &mut emit_fut) {
            (Some(t), Some(e)) => {
                select! {
                    // A) Global shutdown
                    _ = shutdown.cancelled() => {
                        info!("Arbitrage task shutting down");
                        break;
                    },

                    // B) Slow-chain block update
                    Ok(_) = slow_rx.changed() => {
                        let slow = (&*slow_rx.borrow()).clone();
                        info!("Slow chain update received, precomputing arbitrage paths");

                        let pre = strategy.precompute(&slow);
                        last_pre = Some(pre);

                        // Schedule 75% timer
                        let delay = slow_block_interval.mul_f32(0.75);
                        timer = Some(Box::pin(sleep(delay)));
                        info!("Timer scheduled for {:?}", delay);
                    },

                    // C) Timer fires
                    _ = t => {
                        timer.take();
                        info!("Timer fired, computing arbitrage with latest fast chain state");

                        if let Some(pre) = last_pre.take() {
                            let fast = (&*fast_rx.borrow()).clone();
                            let strat = strategy.clone();

                            let fut = async move {
                                if let Some(sig) = strat.compute_arb(&pre, &fast) {
                                    emit_signal(sig).await;
                                }
                            };
                            emit_fut = Some(Box::pin(fut));
                        }
                    },

                    // D) Drive the in-flight emit
                    _ = e => {
                        emit_fut.take();
                        info!("Signal emission completed");
                    },
                }
            },
            (Some(t), None) => {
                select! {
                    _ = shutdown.cancelled() => {
                        info!("Arbitrage task shutting down");
                        break;
                    },
                    Ok(_) = slow_rx.changed() => {
                        let slow = (&*slow_rx.borrow()).clone();
                        info!("Slow chain update received, precomputing arbitrage paths");

                        let pre = strategy.precompute(&slow);
                        last_pre = Some(pre);

                        let delay = slow_block_interval.mul_f32(0.75);
                        timer = Some(Box::pin(sleep(delay)));
                        info!("Timer scheduled for {:?}", delay);
                    },
                    _ = t => {
                        timer.take();
                        info!("Timer fired, computing arbitrage with latest fast chain state");

                        if let Some(pre) = last_pre.take() {
                            let fast = (&*fast_rx.borrow()).clone();
                            let strat = strategy.clone();

                            let fut = async move {
                                if let Some(sig) = strat.compute_arb(&pre, &fast) {
                                    emit_signal(sig).await;
                                }
                            };
                            emit_fut = Some(Box::pin(fut));
                        }
                    },
                }
            },
            (None, Some(e)) => {
                select! {
                    _ = shutdown.cancelled() => {
                        info!("Arbitrage task shutting down");
                        break;
                    },
                    Ok(_) = slow_rx.changed() => {
                        let slow = (&*slow_rx.borrow()).clone();
                        info!("Slow chain update received, precomputing arbitrage paths");

                        let pre = strategy.precompute(&slow);
                        last_pre = Some(pre);

                        let delay = slow_block_interval.mul_f32(0.75);
                        timer = Some(Box::pin(sleep(delay)));
                        info!("Timer scheduled for {:?}", delay);
                    },
                    _ = e => {
                        emit_fut.take();
                        info!("Signal emission completed");
                    },
                }
            },
            (None, None) => {
                select! {
                    _ = shutdown.cancelled() => {
                        info!("Arbitrage task shutting down");
                        break;
                    },
                    Ok(_) = slow_rx.changed() => {
                        let slow = (&*slow_rx.borrow()).clone();
                        info!("Slow chain update received, precomputing arbitrage paths");

                        let pre = strategy.precompute(&slow);
                        last_pre = Some(pre);

                        let delay = slow_block_interval.mul_f32(0.75);
                        timer = Some(Box::pin(sleep(delay)));
                        info!("Timer scheduled for {:?}", delay);
                    },
                }
            },
        }
    }
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
        let slow_state = SlowState {
            state: create_uniswap_v2_state_with_liquidity("950000", "1000000"),
            chain_info: create_test_chain_info("ethereum"),
            block_number: 100,
        };

        // Create fast state (favors token A)
        let fast_state = FastState {
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
            let slow_state = Arc::new(SlowState {
                state: create_uniswap_v2_state_with_liquidity("950000", "1000000"),
                chain_info: create_test_chain_info("ethereum"),
                block_number: 100,
            });

            let fast_state = Arc::new(FastState {
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
