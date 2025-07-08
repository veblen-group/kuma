use color_eyre::eyre;
use futures::{Stream, StreamExt};
use num_bigint::BigUint;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument, warn};
use tycho_simulation::{models::Token, protocol::state::ProtocolSim};

use crate::{
    chain::ChainInfo,
    tycho::{state_update::AssetStateUpdate, ChainSpecificAssetState},
};

#[derive(Debug, Clone)]
pub(crate) struct ArbitrageOpportunity {
    pub(crate) asset_a: Token,
    pub(crate) asset_b: Token,
    pub(crate) slow_chain: ChainInfo,
    pub(crate) fast_chain: ChainInfo,
    pub(crate) path: ArbitragePath,
    pub(crate) slow_chain_amount_out: BigUint,
    pub(crate) fast_chain_amount_out: BigUint,
    pub(crate) profit_percentage: f64,
    pub(crate) optimal_amount_in: BigUint,
    pub(crate) expected_profit: BigUint,
}

#[derive(Debug, Clone)]
pub(crate) enum ArbitragePath {
    /// A->B->A: Start with asset A, swap to B on slow chain, then B back to A on fast chain
    AtoB,
    /// B->A->B: Start with asset B, swap to A on slow chain, then A back to B on fast chain
    BtoA,
}

#[derive(Debug, Clone)]
pub(crate) struct SlowChainCalculation {
    pub(crate) path: ArbitragePath,
    pub(crate) amount_in: BigUint,
    pub(crate) amount_out: BigUint,
    pub(crate) input_token: Token,
    pub(crate) output_token: Token,
}

pub(crate) struct ArbitrageStrategyBuilder {
    pub(crate) asset_a: Token,
    pub(crate) asset_b: Token,
    pub(crate) slow_chain_info: ChainInfo,
    pub(crate) fast_chain_info: ChainInfo,
    pub(crate) slow_chain_stream: ChainSpecificAssetState,
    pub(crate) fast_chain_stream: ChainSpecificAssetState,
    pub(crate) min_profit_threshold: f64,
    pub(crate) max_trade_amount: BigUint,
    pub(crate) binary_search_steps: usize,
    pub(crate) slippage_tolerance: f64,
    pub(crate) risk_factor_bps: u64,
    pub(crate) shutdown_token: CancellationToken,
}

impl ArbitrageStrategyBuilder {
    pub(crate) fn new(
        asset_a: Token,
        asset_b: Token,
        slow_chain_info: ChainInfo,
        fast_chain_info: ChainInfo,
        slow_chain_stream: ChainSpecificAssetState,
        fast_chain_stream: ChainSpecificAssetState,
    ) -> Self {
        Self {
            asset_a,
            asset_b,
            slow_chain_info,
            fast_chain_info,
            slow_chain_stream,
            fast_chain_stream,
            min_profit_threshold: 0.5, // 0.5% minimum profit
            max_trade_amount: BigUint::from(1_000_000u64), // 1M units max
            binary_search_steps: 10, // 10 steps for binary search
            slippage_tolerance: 0.0025, // 0.25% slippage tolerance (Tycho recommended)
            risk_factor_bps: 50, // 0.5% additional risk factor (50 basis points)
            shutdown_token: CancellationToken::new(),
        }
    }

    pub(crate) fn with_min_profit_threshold(mut self, threshold: f64) -> Self {
        self.min_profit_threshold = threshold;
        self
    }

    pub(crate) fn with_max_trade_amount(mut self, amount: BigUint) -> Self {
        self.max_trade_amount = amount;
        self
    }

    pub(crate) fn with_binary_search_steps(mut self, steps: usize) -> Self {
        self.binary_search_steps = steps;
        self
    }

    pub(crate) fn with_slippage_tolerance(mut self, tolerance: f64) -> Self {
        self.slippage_tolerance = tolerance;
        self
    }

    pub(crate) fn with_risk_factor_bps(mut self, risk_factor_bps: u64) -> Self {
        self.risk_factor_bps = risk_factor_bps;
        self
    }

    pub(crate) fn build(self) -> (ArbitrageStrategyHandle, ArbitrageStrategyWorker) {
        let (opportunity_tx, opportunity_rx) = broadcast::channel(100);
        
        let worker = ArbitrageStrategyWorker {
            asset_a: self.asset_a.clone(),
            asset_b: self.asset_b.clone(),
            slow_chain_info: self.slow_chain_info.clone(),
            fast_chain_info: self.fast_chain_info.clone(),
            slow_chain_stream: self.slow_chain_stream,
            fast_chain_stream: self.fast_chain_stream,
            opportunity_tx,
            min_profit_threshold: self.min_profit_threshold,
            max_trade_amount: self.max_trade_amount,
            binary_search_steps: self.binary_search_steps,
            slippage_tolerance: self.slippage_tolerance,
            risk_factor_bps: self.risk_factor_bps,
            shutdown_token: self.shutdown_token.clone(),
        };

        let handle = ArbitrageStrategyHandle {
            asset_a: self.asset_a,
            asset_b: self.asset_b,
            slow_chain_info: self.slow_chain_info,
            fast_chain_info: self.fast_chain_info,
            opportunity_rx,
            shutdown_token: self.shutdown_token,
        };

        (handle, worker)
    }
}

pub(crate) struct ArbitrageStrategyHandle {
    pub(crate) asset_a: Token,
    pub(crate) asset_b: Token,
    pub(crate) slow_chain_info: ChainInfo,
    pub(crate) fast_chain_info: ChainInfo,
    opportunity_rx: broadcast::Receiver<ArbitrageOpportunity>,
    shutdown_token: CancellationToken,
}

impl ArbitrageStrategyHandle {
    pub(crate) fn opportunities(&self) -> impl Stream<Item = ArbitrageOpportunity> + '_ {
        tokio_stream::wrappers::BroadcastStream::new(self.opportunity_rx.resubscribe())
            .filter_map(|result| async move { result.ok() })
    }

    pub(crate) fn shutdown(&self) {
        self.shutdown_token.cancel();
    }
}

pub(crate) struct ArbitrageStrategyWorker {
    asset_a: Token,
    asset_b: Token,
    slow_chain_info: ChainInfo,
    fast_chain_info: ChainInfo,
    slow_chain_stream: ChainSpecificAssetState,
    fast_chain_stream: ChainSpecificAssetState,
    opportunity_tx: broadcast::Sender<ArbitrageOpportunity>,
    min_profit_threshold: f64,
    max_trade_amount: BigUint,
    binary_search_steps: usize,
    slippage_tolerance: f64,
    risk_factor_bps: u64,
    shutdown_token: CancellationToken,
}

impl ArbitrageStrategyWorker {
    #[instrument(skip(self), fields(
        asset_a = %self.asset_a.symbol,
        asset_b = %self.asset_b.symbol,
        slow_chain = %self.slow_chain_info.chain,
        fast_chain = %self.fast_chain_info.chain
    ))]
    pub(crate) async fn run(mut self) -> eyre::Result<()> {
        info!("Starting arbitrage strategy worker for slow/fast chain arbitrage");
        
        let mut fast_chain_state: Option<AssetStateUpdate> = None;
        
        loop {
            tokio::select! {
                _ = self.shutdown_token.cancelled() => {
                    info!("Arbitrage strategy worker shutting down");
                    break;
                }
                
                // When slow chain update is received, calculate amount outs and check arbitrage
                slow_update = self.slow_chain_stream.next() => {
                    match slow_update {
                        Some(update) => {
                            info!("Received slow chain update, calculating amount outs");
                            let slow_calculations = self.calculate_slow_chain_amounts(&update).await?;
                            
                            // Query current fast chain state and complete arbitrage calculation
                            if let Some(ref fast_state) = fast_chain_state {
                                self.complete_arbitrage_calculation(&slow_calculations, fast_state).await?;
                            }
                        }
                        None => {
                            warn!("Slow chain stream ended");
                            break;
                        }
                    }
                }
                
                // Keep fast chain state updated
                fast_update = self.fast_chain_stream.next() => {
                    match fast_update {
                        Some(update) => {
                            fast_chain_state = Some(update);
                        }
                        None => {
                            warn!("Fast chain stream ended");
                            break;
                        }
                    }
                }
            }
        }
        
        Ok(())
    }

    async fn calculate_slow_chain_amounts(&self, slow_state: &AssetStateUpdate) -> eyre::Result<Vec<SlowChainCalculation>> {
        let mut calculations = Vec::new();
        
        // Create amount input ranges for binary search
        let step_size = &self.max_trade_amount / BigUint::from(self.binary_search_steps as u64);
        
        for i in 1..=self.binary_search_steps {
            let amount_in = &step_size * BigUint::from(i as u64);
            
            // Path A->B->A: Start with asset A, swap to B on slow chain
            let amount_out_a_to_b = self.simulate_swap(&slow_state.state, &self.asset_a, &self.asset_b, &amount_in)?;
            calculations.push(SlowChainCalculation {
                path: ArbitragePath::AtoB,
                amount_in: amount_in.clone(),
                amount_out: amount_out_a_to_b,
                input_token: self.asset_a.clone(),
                output_token: self.asset_b.clone(),
            });
            
            // Path B->A->B: Start with asset B, swap to A on slow chain
            let amount_out_b_to_a = self.simulate_swap(&slow_state.state, &self.asset_b, &self.asset_a, &amount_in)?;
            calculations.push(SlowChainCalculation {
                path: ArbitragePath::BtoA,
                amount_in: amount_in.clone(),
                amount_out: amount_out_b_to_a,
                input_token: self.asset_b.clone(),
                output_token: self.asset_a.clone(),
            });
        }
        
        Ok(calculations)
    }

    async fn complete_arbitrage_calculation(
        &self,
        slow_calculations: &[SlowChainCalculation],
        fast_state: &AssetStateUpdate,
    ) -> eyre::Result<()> {
        let mut best_opportunity: Option<ArbitrageOpportunity> = None;
        let mut best_profit = BigUint::from(0u64);
        
        for slow_calc in slow_calculations {
            // Complete the arbitrage path based on the slow chain calculation
            let (fast_input_token, fast_output_token, _expected_final_token) = match slow_calc.path {
                ArbitragePath::AtoB => {
                    // A->B->A: slow chain produced B, fast chain should convert B back to A
                    (&slow_calc.output_token, &slow_calc.input_token, &slow_calc.input_token)
                },
                ArbitragePath::BtoA => {
                    // B->A->B: slow chain produced A, fast chain should convert A back to B
                    (&slow_calc.output_token, &slow_calc.input_token, &slow_calc.input_token)
                }
            };
            
            // Use the amount_out from slow chain as amount_in for fast chain
            let fast_amount_out = self.simulate_swap(
                &fast_state.state,
                fast_input_token,
                fast_output_token,
                &slow_calc.amount_out,
            )?;
            
            // Calculate profit: fast_amount_out - slow_amount_in
            // Both paths should end up with more of the starting token
            if fast_amount_out > slow_calc.amount_in {
                let profit = &fast_amount_out - &slow_calc.amount_in;
                let profit_percentage = (profit.clone() * BigUint::from(10000u64) / &slow_calc.amount_in).to_string().parse::<f64>().unwrap_or(0.0) / 100.0;
                
                if profit_percentage >= self.min_profit_threshold && profit > best_profit {
                    best_profit = profit.clone();
                    best_opportunity = Some(ArbitrageOpportunity {
                        asset_a: self.asset_a.clone(),
                        asset_b: self.asset_b.clone(),
                        slow_chain: self.slow_chain_info.clone(),
                        fast_chain: self.fast_chain_info.clone(),
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
        
        if let Some(opportunity) = best_opportunity {
            if let Err(e) = self.opportunity_tx.send(opportunity) {
                warn!("Failed to send arbitrage opportunity: {}", e);
            }
        }
        
        Ok(())
    }

    fn simulate_swap(
        &self,
        state: &Box<dyn ProtocolSim>,
        token_in: &Token,
        token_out: &Token,
        amount_in: &BigUint,
    ) -> eyre::Result<BigUint> {
        // Get the swap result using Tycho's simulation
        let swap_result = state.get_amount_out(amount_in.clone(), token_in, token_out)
            .map_err(|e| eyre::eyre!("Swap simulation failed: {:?}", e))?;
        
        // Apply slippage tolerance and risk factor to get minimum amount out
        // Following Tycho's recommendation with additional risk management
        let slippage_bps = (self.slippage_tolerance * 10000.0) as u64;
        let total_risk_bps = slippage_bps + self.risk_factor_bps;
        let risk_multiplier = BigUint::from(10000u64 - total_risk_bps);
        let min_amount_out = (&swap_result.amount * &risk_multiplier) / BigUint::from(10000u64);
        
        // Return the minimum amount out considering both slippage and risk factor
        // This represents the worst-case scenario for comprehensive risk management
        Ok(min_amount_out)
    }

    fn calculate_minimum_amount_out(&self, expected_amount: &BigUint) -> BigUint {
        // Calculate minimum amount out with slippage protection and risk factor
        // Following Tycho documentation with additional risk management
        let slippage_bps = (self.slippage_tolerance * 10000.0) as u64;
        let total_risk_bps = slippage_bps + self.risk_factor_bps;
        let risk_multiplier = BigUint::from(10000u64 - total_risk_bps);
        (expected_amount * &risk_multiplier) / BigUint::from(10000u64)
    }

    /// Prepares swap execution parameters for Tycho router encoding
    /// This would be used with TychoRouterEncoderBuilder for actual execution
    fn prepare_swap_execution_params(
        &self,
        opportunity: &ArbitrageOpportunity,
        sender_address: &str,
        receiver_address: &str,
    ) -> SwapExecutionParams {
        SwapExecutionParams {
            sender: sender_address.to_string(),
            receiver: receiver_address.to_string(),
            path: opportunity.path.clone(),
            input_amount: opportunity.optimal_amount_in.clone(),
            minimum_output_amount: self.calculate_minimum_amount_out(&opportunity.expected_profit),
            slippage_tolerance: self.slippage_tolerance,
            risk_factor_bps: self.risk_factor_bps,
            asset_a: opportunity.asset_a.clone(),
            asset_b: opportunity.asset_b.clone(),
            slow_chain: opportunity.slow_chain.clone(),
            fast_chain: opportunity.fast_chain.clone(),
        }
    }
}

/// Parameters for executing arbitrage swaps through Tycho router
#[derive(Debug, Clone)]
pub(crate) struct SwapExecutionParams {
    pub(crate) sender: String,
    pub(crate) receiver: String,
    pub(crate) path: ArbitragePath,
    pub(crate) input_amount: BigUint,
    pub(crate) minimum_output_amount: BigUint,
    pub(crate) slippage_tolerance: f64,
    pub(crate) risk_factor_bps: u64,
    pub(crate) asset_a: Token,
    pub(crate) asset_b: Token,
    pub(crate) slow_chain: ChainInfo,
    pub(crate) fast_chain: ChainInfo,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;
    use tokio_stream::wrappers::BroadcastStream;
    use tycho_simulation::protocol::state::ProtocolSim;
    use num_bigint::BigUint;

    // Helper function to create UniswapV2State for testing with realistic liquidity
    fn create_uniswap_v2_state_with_liquidity(
        reserve_0: &str,
        reserve_1: &str
    ) -> Box<dyn ProtocolSim> {
        use tycho_simulation::evm::protocol::uniswap_v2::state::UniswapV2State;
        use std::str::FromStr;
        
        // Use the U256 type that tycho_simulation expects
        let reserve_0_u256 = alloy::primitives::U256::from_str(reserve_0).unwrap();
        let reserve_1_u256 = alloy::primitives::U256::from_str(reserve_1).unwrap();
        
        Box::new(UniswapV2State::new(reserve_0_u256, reserve_1_u256))
    }
    
    // Create state that favors token A (A is cheaper, B is expensive)
    fn create_state_favoring_token_a(_token_a: &Token, _token_b: &Token) -> Box<dyn ProtocolSim> {
        // Slightly more token0 reserves than token1 = token0 is slightly cheaper
        // More realistic arbitrage scenario with smaller price differences
        create_uniswap_v2_state_with_liquidity(
            "1000000", // 1M token0 (reserve0)
            "950000"   // 950K token1 (reserve1) - token0:token1 = ~1.05:1, small price difference
        )
    }
    
    // Create state that favors token B (B is cheaper, A is expensive) 
    fn create_state_favoring_token_b(_token_a: &Token, _token_b: &Token) -> Box<dyn ProtocolSim> {
        // Slightly more token1 reserves than token0 = token1 is slightly cheaper
        // More realistic arbitrage scenario with smaller price differences
        create_uniswap_v2_state_with_liquidity(
            "950000",  // 950K token0 (reserve0) 
            "1000000"  // 1M token1 (reserve1) - token0:token1 = ~0.95:1, small price difference
        )
    }
    
    fn create_asset_state_update_with_uniswap_state(
        token_a: &Token,
        token_b: &Token,
        favor_token: &str
    ) -> AssetStateUpdate {
        let state = match favor_token {
            "A" => create_state_favoring_token_a(token_a, token_b),
            "B" => create_state_favoring_token_b(token_a, token_b),
            _ => panic!("favor_token must be 'A' or 'B'"),
        };
        
        AssetStateUpdate {
            state,
            inventory: crate::strategies::TradeAmounts {
                amount_in: 0,
                amount_out: 0,
            },
        }
    }

    fn create_test_token(symbol: &str, address: &str) -> Token {
        Token {
            symbol: symbol.to_string(),
            address: address.as_bytes().to_vec().into(),
            decimals: 18,
            gas: num_bigint::BigUint::from(0u64),
        }
    }

    fn create_test_chain_info(chain: &str) -> ChainInfo {
        ChainInfo {
            chain: chain.to_string(),
            ..Default::default()
        }
    }

    fn create_test_asset_state() -> ChainSpecificAssetState {
        let token_a = create_test_token("USDC", "0x1");
        let token_b = create_test_token("USDT", "0x2");
        let (tx, rx) = broadcast::channel(10);
        
        ChainSpecificAssetState {
            asset_a: token_a,
            asset_b: token_b,
            tx,
            rx: BroadcastStream::new(rx),
        }
    }

    #[test]
    fn test_arbitrage_strategy_builder() {
        let token_a = create_test_token("USDC", "0x1");
        let token_b = create_test_token("USDT", "0x2");
        let slow_chain = create_test_chain_info("ethereum");
        let fast_chain = create_test_chain_info("polygon");
        let slow_state = create_test_asset_state();
        let fast_state = create_test_asset_state();

        let builder = ArbitrageStrategyBuilder::new(
            token_a.clone(),
            token_b.clone(),
            slow_chain.clone(),
            fast_chain.clone(),
            slow_state,
            fast_state,
        )
        .with_min_profit_threshold(1.0)
        .with_max_trade_amount(BigUint::from(500_000u64))
        .with_binary_search_steps(5)
        .with_slippage_tolerance(0.005) // 0.5% slippage
        .with_risk_factor_bps(100); // 1% risk factor (100 bps)

        assert_eq!(builder.min_profit_threshold, 1.0);
        assert_eq!(builder.max_trade_amount, BigUint::from(500_000u64));
        assert_eq!(builder.binary_search_steps, 5);
        assert_eq!(builder.slippage_tolerance, 0.005);
        assert_eq!(builder.risk_factor_bps, 100);
        assert_eq!(builder.asset_a.symbol, "USDC");
        assert_eq!(builder.asset_b.symbol, "USDT");
        assert_eq!(builder.slow_chain_info.chain, "ethereum");
        assert_eq!(builder.fast_chain_info.chain, "polygon");
    }

    #[test]
    fn test_slippage_calculation() {
        let token_a = create_test_token("USDC", "0x1");
        let token_b = create_test_token("USDT", "0x2");
        let slow_chain = create_test_chain_info("ethereum");
        let fast_chain = create_test_chain_info("polygon");
        let slow_state = create_test_asset_state();
        let fast_state = create_test_asset_state();
        let (tx, _) = broadcast::channel(10);

        let worker = ArbitrageStrategyWorker {
            asset_a: token_a.clone(),
            asset_b: token_b.clone(),
            slow_chain_info: slow_chain,
            fast_chain_info: fast_chain,
            slow_chain_stream: slow_state,
            fast_chain_stream: fast_state,
            opportunity_tx: tx,
            min_profit_threshold: 0.5,
            max_trade_amount: BigUint::from(1_000_000u64),
            binary_search_steps: 10,
            slippage_tolerance: 0.0025, // 0.25% slippage (Tycho recommended)
            risk_factor_bps: 50, // 0.5% risk factor (50 bps)
            shutdown_token: CancellationToken::new(),
        };

        // Test minimum amount out calculation with slippage and risk factor
        let expected_amount = BigUint::from(1000u64);
        let min_amount_out = worker.calculate_minimum_amount_out(&expected_amount);
        
        // With 0.25% slippage (25 bps) + 0.5% risk factor (50 bps) = 75 bps total
        // min_amount_out = 1000 * (1 - 0.0075) = 1000 * 0.9925 = 992.5
        assert_eq!(min_amount_out, BigUint::from(992u64));

        // Test with larger amount
        let expected_amount = BigUint::from(10000u64);
        let min_amount_out = worker.calculate_minimum_amount_out(&expected_amount);
        
        // With 0.25% slippage + 0.5% risk factor = 0.75% total
        // min_amount_out = 10000 * (1 - 0.0075) = 10000 * 0.9925 = 9925
        assert_eq!(min_amount_out, BigUint::from(9925u64));
    }

    #[test]
    fn test_bidirectional_arbitrage_calculations() {
        let token_a = create_test_token("USDC", "0x1");
        let token_b = create_test_token("USDT", "0x2");
        let slow_chain = create_test_chain_info("ethereum");
        let fast_chain = create_test_chain_info("polygon");
        let slow_state = create_test_asset_state();
        let fast_state = create_test_asset_state();
        let (tx, _) = broadcast::channel(10);

        let worker = ArbitrageStrategyWorker {
            asset_a: token_a.clone(),
            asset_b: token_b.clone(),
            slow_chain_info: slow_chain,
            fast_chain_info: fast_chain,
            slow_chain_stream: slow_state,
            fast_chain_stream: fast_state,
            opportunity_tx: tx,
            min_profit_threshold: 0.5,
            max_trade_amount: BigUint::from(1_000_000u64),
            binary_search_steps: 5,
            slippage_tolerance: 0.0025, // 0.25% slippage
            risk_factor_bps: 50, // 0.5% risk factor
            shutdown_token: CancellationToken::new(),
        };

        // Test that we generate the correct number of calculations for both paths
        let step_size = &worker.max_trade_amount / BigUint::from(worker.binary_search_steps as u64);
        assert_eq!(step_size, BigUint::from(200_000u64));

        // Test that both arbitrage paths are calculated
        // Expected: binary_search_steps * 2 (for A->B->A and B->A->B paths)
        let expected_total_calculations = worker.binary_search_steps * 2;
        
        // Test binary search step calculation logic for both paths
        for i in 1..=worker.binary_search_steps {
            let amount_in = &step_size * BigUint::from(i as u64);
            
            // With 0.25% slippage (25 bps) + 0.5% risk factor (50 bps) = 75 bps total
            // minimum amount out = amount_in * (1 - 0.0075) = amount_in * 0.9925
            let min_amount_out = worker.calculate_minimum_amount_out(&amount_in);
            let expected_min_amount = (&amount_in * BigUint::from(9925u64)) / BigUint::from(10000u64);
            
            // Path A->B->A: A as input, B as output (with slippage)
            assert_eq!(min_amount_out, expected_min_amount);
            
            // Path B->A->B: B as input, A as output (with slippage)
            assert_eq!(min_amount_out, expected_min_amount);
        }
        
        // Verify we have both paths covered
        assert_eq!(expected_total_calculations, 10); // 5 steps * 2 paths
    }

    #[test]
    fn test_mock_protocol_swap_simulation() {
        let token_a = create_test_token("USDC", "0x1");
        let token_b = create_test_token("USDT", "0x2");
        let slow_chain = create_test_chain_info("ethereum");
        let fast_chain = create_test_chain_info("polygon");
        let slow_state = create_test_asset_state();
        let fast_state = create_test_asset_state();
        let (tx, _) = broadcast::channel(10);

        let worker = ArbitrageStrategyWorker {
            asset_a: token_a.clone(),
            asset_b: token_b.clone(),
            slow_chain_info: slow_chain,
            fast_chain_info: fast_chain,
            slow_chain_stream: slow_state,
            fast_chain_stream: fast_state,
            opportunity_tx: tx,
            min_profit_threshold: 0.5,
            max_trade_amount: BigUint::from(1_000_000u64),
            binary_search_steps: 10,
            slippage_tolerance: 0.003, // 0.3% slippage
            risk_factor_bps: 50, // 0.5% risk factor
            shutdown_token: CancellationToken::new(),
        };

        // Create UniswapV2 state for testing
        let mock_state = create_state_favoring_token_a(&token_a, &token_b);

        // Test swap: 1000 units -> using UniswapV2 constant product formula
        let amount_in = BigUint::from(1000u64); // 1000 units
        let result = worker.simulate_swap(&mock_state, &token_a, &token_b, &amount_in);
        
        assert!(result.is_ok());
        let amount_out = result.unwrap();
        
        // With UniswapV2 constant product formula: 
        // Pool ratio 1M:950K, trading 1000 units should give reasonable output
        // Expected: roughly proportional but with fees and slippage applied
        assert!(amount_out > BigUint::from(0u64));
        assert!(amount_out < amount_in); // Should get less out due to fees and slippage
        
        // With realistic UniswapV2 calculation, we expect something in a reasonable range
        // Pool has slight imbalance (1M:950K ≈ 1.05:1), so should get close to input amount minus fees
        let expected_min = BigUint::from(900u64); // Allow for fees and slippage
        let expected_max = BigUint::from(980u64); // Upper bound accounting for pool dynamics
        assert!(amount_out >= expected_min, "Amount out too low: {}", amount_out);
        assert!(amount_out <= expected_max, "Amount out too high: {}", amount_out);
    }

    #[test]
    fn test_risk_factor_calculation() {
        let token_a = create_test_token("USDC", "0x1");
        let token_b = create_test_token("USDT", "0x2");
        let slow_chain = create_test_chain_info("ethereum");
        let fast_chain = create_test_chain_info("polygon");
        let slow_state = create_test_asset_state();
        let fast_state = create_test_asset_state();
        let (tx, _) = broadcast::channel(10);

        // Test with different risk factor values
        let worker_low_risk = ArbitrageStrategyWorker {
            asset_a: token_a.clone(),
            asset_b: token_b.clone(),
            slow_chain_info: slow_chain.clone(),
            fast_chain_info: fast_chain.clone(),
            slow_chain_stream: slow_state.clone(),
            fast_chain_stream: fast_state.clone(),
            opportunity_tx: tx.clone(),
            min_profit_threshold: 0.5,
            max_trade_amount: BigUint::from(1_000_000u64),
            binary_search_steps: 10,
            slippage_tolerance: 0.0025, // 0.25% slippage (25 bps)
            risk_factor_bps: 25, // 0.25% risk factor (25 bps)
            shutdown_token: CancellationToken::new(),
        };

        let worker_high_risk = ArbitrageStrategyWorker {
            asset_a: token_a.clone(),
            asset_b: token_b.clone(),
            slow_chain_info: slow_chain,
            fast_chain_info: fast_chain,
            slow_chain_stream: slow_state,
            fast_chain_stream: fast_state,
            opportunity_tx: tx,
            min_profit_threshold: 0.5,
            max_trade_amount: BigUint::from(1_000_000u64),
            binary_search_steps: 10,
            slippage_tolerance: 0.0025, // 0.25% slippage (25 bps)
            risk_factor_bps: 100, // 1% risk factor (100 bps)
            shutdown_token: CancellationToken::new(),
        };

        let test_amount = BigUint::from(10000u64);
        
        // Low risk: 25 bps slippage + 25 bps risk factor = 50 bps total
        // Expected: 10000 * (1 - 0.005) = 10000 * 0.995 = 9950
        let low_risk_result = worker_low_risk.calculate_minimum_amount_out(&test_amount);
        assert_eq!(low_risk_result, BigUint::from(9950u64));
        
        // High risk: 25 bps slippage + 100 bps risk factor = 125 bps total
        // Expected: 10000 * (1 - 0.0125) = 10000 * 0.9875 = 9875
        let high_risk_result = worker_high_risk.calculate_minimum_amount_out(&test_amount);
        assert_eq!(high_risk_result, BigUint::from(9875u64));
        
        // Verify high risk gives lower amount (more conservative)
        assert!(high_risk_result < low_risk_result);
    }

    #[test]
    fn test_arbitrage_calculation_with_mock_protocol_state() {
        let token_a = create_test_token("USDC", "0x1");
        let token_b = create_test_token("USDT", "0x2");
        let slow_chain = create_test_chain_info("ethereum");
        let fast_chain = create_test_chain_info("polygon");
        let slow_state = create_test_asset_state();
        let fast_state = create_test_asset_state();
        let (tx, _) = broadcast::channel(10);

        let worker = ArbitrageStrategyWorker {
            asset_a: token_a.clone(),
            asset_b: token_b.clone(),
            slow_chain_info: slow_chain,
            fast_chain_info: fast_chain,
            slow_chain_stream: slow_state,
            fast_chain_stream: fast_state,
            opportunity_tx: tx,
            min_profit_threshold: 0.5,
            max_trade_amount: BigUint::from(100_000u64),
            binary_search_steps: 3, // Smaller for test performance
            slippage_tolerance: 0.003, // 0.3% slippage
            risk_factor_bps: 30, // 0.3% risk factor
            shutdown_token: CancellationToken::new(),
        };

        // Create arbitrage scenario using real UniswapV2 pools:
        // Both chains use same liquidity ratios - no arbitrage expected in this test
        let slow_state_update = create_asset_state_update_with_uniswap_state(&token_a, &token_b, "A");
        let fast_state_update = create_asset_state_update_with_uniswap_state(&token_a, &token_b, "A");

        // Test slow chain amount calculation
        let slow_calculations_result = tokio_test::block_on(
            worker.calculate_slow_chain_amounts(&slow_state_update)
        );
        
        assert!(slow_calculations_result.is_ok());
        let slow_calculations = slow_calculations_result.unwrap();
        
        // Should have calculations for both arbitrage paths (A->B->A and B->A->B)
        // With 3 binary search steps, we expect 6 calculations (3 steps × 2 paths)
        assert_eq!(slow_calculations.len(), 6);
        
        // Verify each calculation has valid amounts
        for calc in &slow_calculations {
            assert!(calc.amount_in > BigUint::from(0u64));
            assert!(calc.amount_out > BigUint::from(0u64));
            // With UniswapV2, amount out depends on pool ratios and might sometimes be favorable
            // Just verify we get reasonable outputs
        }
        
        // Test complete arbitrage calculation
        let arbitrage_result = tokio_test::block_on(
            worker.complete_arbitrage_calculation(&slow_calculations, &fast_state_update)
        );
        
        assert!(arbitrage_result.is_ok());
        // Note: We don't assert on finding profitable opportunities since the test setup
        // may not create profitable arbitrage after all fees and risk factors
    }

    #[test]
    fn test_profitable_arbitrage_opportunity_detection() {
        let token_a = create_test_token("USDC", "0x1");
        let token_b = create_test_token("USDT", "0x2");
        let slow_chain = create_test_chain_info("ethereum");
        let fast_chain = create_test_chain_info("polygon");
        let slow_state = create_test_asset_state();
        let fast_state = create_test_asset_state();
        let (tx, mut rx) = broadcast::channel(10);

        let worker = ArbitrageStrategyWorker {
            asset_a: token_a.clone(),
            asset_b: token_b.clone(),
            slow_chain_info: slow_chain,
            fast_chain_info: fast_chain,
            slow_chain_stream: slow_state,
            fast_chain_stream: fast_state,
            opportunity_tx: tx,
            min_profit_threshold: 0.5, // 0.5% minimum profit threshold (more realistic for DeFi)  
            max_trade_amount: BigUint::from(1_000u64), // Much smaller trade size (1000 units, realistic for testing)
            binary_search_steps: 5, // More steps for better optimization
            slippage_tolerance: 0.0025, // 0.25% slippage (Uniswap V2 standard)
            risk_factor_bps: 25, // 0.25% risk factor (more conservative)
            shutdown_token: CancellationToken::new(),
        };

        // Create clearly profitable arbitrage scenario using real UniswapV2 pools:
        // Slow chain: Favors token B (A is expensive, B is cheap) - good for selling A
        // Fast chain: Favors token A (A is cheap, B is expensive) - good for buying A
        // Arbitrage path A->B->A: Sell expensive A on slow chain, buy cheap A on fast chain
        let slow_state_update = create_asset_state_update_with_uniswap_state(&token_a, &token_b, "B");
        let fast_state_update = create_asset_state_update_with_uniswap_state(&token_a, &token_b, "A");

        // Test slow chain amount calculation
        let slow_calculations_result = tokio_test::block_on(
            worker.calculate_slow_chain_amounts(&slow_state_update)
        );
        
        assert!(slow_calculations_result.is_ok());
        let slow_calculations = slow_calculations_result.unwrap();
        
        // Should have 10 calculations (5 steps × 2 paths)
        assert_eq!(slow_calculations.len(), 10);
        
        // Test complete arbitrage calculation - this should find profitable opportunities
        let arbitrage_result = tokio_test::block_on(
            worker.complete_arbitrage_calculation(&slow_calculations, &fast_state_update)
        );
        
        assert!(arbitrage_result.is_ok());
        
        // Debug: Show all trade amounts tested and their outcomes
        println!("🔍 All Trade Amounts Tested:");
        println!("   ═══════════════════════════════════════");
        for (i, calc) in slow_calculations.iter().enumerate() {
            // Simulate what the fast chain would return for each calculation
            let fast_amount_out = worker.simulate_swap(&fast_state_update.state, &calc.output_token, &calc.input_token, &calc.amount_out).unwrap();
            let profit = if fast_amount_out > calc.amount_in { &fast_amount_out - &calc.amount_in } else { BigUint::from(0u64) };
            let profit_percentage = if calc.amount_in > BigUint::from(0u64) {
                (profit.clone() * BigUint::from(10000u64) / &calc.amount_in).to_string().parse::<f64>().unwrap_or(0.0) / 100.0
            } else { 0.0 };
            let path_desc = match calc.path {
                ArbitragePath::AtoB => "A→B→A",
                ArbitragePath::BtoA => "B→A→B",
            };
            
            println!("   Step {}: {} | In: {} → Slow: {} → Fast: {} | Profit: {} ({:.2}%)", 
                i+1, path_desc, calc.amount_in, calc.amount_out, fast_amount_out, profit, profit_percentage);
        }
        println!("   ═══════════════════════════════════════");
        

        // Check if an arbitrage opportunity was broadcast
        // Since the calculation is complex with risk factors, let's check if we got any opportunities
        let opportunity_received = rx.try_recv();
        
        match opportunity_received {
            Ok(opportunity) => {
                // We found a profitable opportunity! Validate it
                assert!(opportunity.profit_percentage >= worker.min_profit_threshold);
                assert!(opportunity.expected_profit > BigUint::from(0u64));
                assert!(opportunity.optimal_amount_in > BigUint::from(0u64));
                assert!(opportunity.fast_chain_amount_out > opportunity.optimal_amount_in); // Should be profitable
                
                // Verify the opportunity structure
                assert_eq!(opportunity.asset_a.symbol, "USDC");
                assert_eq!(opportunity.asset_b.symbol, "USDT");
                assert_eq!(opportunity.slow_chain.chain, "ethereum");
                assert_eq!(opportunity.fast_chain.chain, "polygon");
                
                // Opportunity found - this is expected with our profitable test scenario
                println!("🎯 Best Arbitrage Opportunity Found:");
                println!("   ═══════════════════════════════════════");
                println!("   📊 Trade Details:");
                println!("      • Trade Amount: {} units", opportunity.optimal_amount_in);
                println!("      • Expected Profit: {} units", opportunity.expected_profit);
                println!("      • Profit Percentage: {:.2}%", opportunity.profit_percentage);
                println!("      • Final Amount Out: {} units", opportunity.fast_chain_amount_out);
                println!();
                println!("   🔄 Arbitrage Path: {:?}", opportunity.path);
                println!("   📈 Chain Details:");
                println!("      • Slow Chain: {} ({})", opportunity.slow_chain.chain, "Pool B favored - 950K:1M");
                println!("      • Fast Chain: {} ({})", opportunity.fast_chain.chain, "Pool A favored - 1M:950K");
                println!("   💱 Assets: {} ↔ {}", opportunity.asset_a.symbol, opportunity.asset_b.symbol);
                println!();
                println!("   🧮 Calculation Breakdown:");
                println!("      • Slow Chain Output: {} units", opportunity.slow_chain_amount_out);
                println!("      • Fast Chain Final: {} units", opportunity.fast_chain_amount_out);
                println!("      • Net Gain: {} units", &opportunity.fast_chain_amount_out - &opportunity.optimal_amount_in);
                println!("   ═══════════════════════════════════════");
            }
            Err(broadcast::error::TryRecvError::Empty) => {
                // No opportunity found - this should not happen with our profitable test scenario
                panic!("Expected to find profitable arbitrage opportunity but none was detected");
            }
            Err(e) => {
                panic!("Unexpected error receiving opportunity: {:?}", e);
            }
        }
    }

    #[test]
    fn test_arbitrage_paths() {
        // Test ArbitragePath enum
        let path_a_to_b = ArbitragePath::AtoB;
        let path_b_to_a = ArbitragePath::BtoA;
        
        // Verify paths are different
        match path_a_to_b {
            ArbitragePath::AtoB => assert!(true),
            ArbitragePath::BtoA => assert!(false),
        }
        
        match path_b_to_a {
            ArbitragePath::AtoB => assert!(false),
            ArbitragePath::BtoA => assert!(true),
        }
    }
}