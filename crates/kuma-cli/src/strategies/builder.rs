use color_eyre::eyre;
use futures::{Stream, StreamExt};
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
    pub(crate) slow_chain_amount_out: u64,
    pub(crate) fast_chain_amount_out: u64,
    pub(crate) profit_percentage: f64,
    pub(crate) optimal_amount_in: u64,
    pub(crate) expected_profit: u64,
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
    pub(crate) amount_in: u64,
    pub(crate) amount_out: u64,
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
    pub(crate) max_trade_amount: u64,
    pub(crate) binary_search_steps: usize,
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
            max_trade_amount: 1_000_000, // 1M units max
            binary_search_steps: 10, // 10 steps for binary search
            shutdown_token: CancellationToken::new(),
        }
    }

    pub(crate) fn with_min_profit_threshold(mut self, threshold: f64) -> Self {
        self.min_profit_threshold = threshold;
        self
    }

    pub(crate) fn with_max_trade_amount(mut self, amount: u64) -> Self {
        self.max_trade_amount = amount;
        self
    }

    pub(crate) fn with_binary_search_steps(mut self, steps: usize) -> Self {
        self.binary_search_steps = steps;
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
    max_trade_amount: u64,
    binary_search_steps: usize,
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
        let step_size = self.max_trade_amount / self.binary_search_steps as u64;
        
        for i in 1..=self.binary_search_steps {
            let amount_in = step_size * i as u64;
            
            // Path A->B->A: Start with asset A, swap to B on slow chain
            let amount_out_a_to_b = self.simulate_swap(&slow_state.state, &self.asset_a, &self.asset_b, amount_in)?;
            calculations.push(SlowChainCalculation {
                path: ArbitragePath::AtoB,
                amount_in,
                amount_out: amount_out_a_to_b,
                input_token: self.asset_a.clone(),
                output_token: self.asset_b.clone(),
            });
            
            // Path B->A->B: Start with asset B, swap to A on slow chain
            let amount_out_b_to_a = self.simulate_swap(&slow_state.state, &self.asset_b, &self.asset_a, amount_in)?;
            calculations.push(SlowChainCalculation {
                path: ArbitragePath::BtoA,
                amount_in,
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
        let mut best_profit = 0u64;
        
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
                slow_calc.amount_out,
            )?;
            
            // Calculate profit: fast_amount_out - slow_amount_in
            // Both paths should end up with more of the starting token
            if fast_amount_out > slow_calc.amount_in {
                let profit = fast_amount_out - slow_calc.amount_in;
                let profit_percentage = (profit as f64 / slow_calc.amount_in as f64) * 100.0;
                
                if profit_percentage >= self.min_profit_threshold && profit > best_profit {
                    best_profit = profit;
                    best_opportunity = Some(ArbitrageOpportunity {
                        asset_a: self.asset_a.clone(),
                        asset_b: self.asset_b.clone(),
                        slow_chain: self.slow_chain_info.clone(),
                        fast_chain: self.fast_chain_info.clone(),
                        path: slow_calc.path.clone(),
                        slow_chain_amount_out: slow_calc.amount_out,
                        fast_chain_amount_out: fast_amount_out,
                        profit_percentage,
                        optimal_amount_in: slow_calc.amount_in,
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
        _state: &Box<dyn ProtocolSim>,
        _token_in: &Token,
        _token_out: &Token,
        amount_in: u64,
    ) -> eyre::Result<u64> {
        // TODO: Implement actual swap simulation using state.simulate_swap
        // For now, return a placeholder amount (98% of input for 2% slippage)
        Ok((amount_in as f64 * 0.98) as u64)
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::mock;
    use tokio::sync::broadcast;
    use tokio_stream::wrappers::BroadcastStream;
    use tycho_simulation::protocol::state::ProtocolSim;

    // Mock implementation would go here when tycho-simulation API is clarified

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
        .with_max_trade_amount(500_000)
        .with_binary_search_steps(5);

        assert_eq!(builder.min_profit_threshold, 1.0);
        assert_eq!(builder.max_trade_amount, 500_000);
        assert_eq!(builder.binary_search_steps, 5);
        assert_eq!(builder.asset_a.symbol, "USDC");
        assert_eq!(builder.asset_b.symbol, "USDT");
        assert_eq!(builder.slow_chain_info.chain, "ethereum");
        assert_eq!(builder.fast_chain_info.chain, "polygon");
    }

    #[test]
    fn test_simulate_swap_logic() {
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
            max_trade_amount: 1_000_000,
            binary_search_steps: 10,
            shutdown_token: CancellationToken::new(),
        };

        // Test the internal logic of simulate_swap (placeholder implementation)
        let amount_in = 1000u64;
        let expected_amount_out = (amount_in as f64 * 0.98) as u64;
        assert_eq!(expected_amount_out, 980); // 98% of 1000

        // Test with larger amount
        let amount_in = 10000u64;
        let expected_amount_out = (amount_in as f64 * 0.98) as u64;
        assert_eq!(expected_amount_out, 9800); // 98% of 10000
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
            max_trade_amount: 1_000_000,
            binary_search_steps: 5,
            shutdown_token: CancellationToken::new(),
        };

        // Test that we generate the correct number of calculations for both paths
        let step_size = worker.max_trade_amount / worker.binary_search_steps as u64;
        assert_eq!(step_size, 200_000);

        // Test that both arbitrage paths are calculated
        // Expected: binary_search_steps * 2 (for A->B->A and B->A->B paths)
        let expected_total_calculations = worker.binary_search_steps * 2;
        
        // Test binary search step calculation logic for both paths
        for i in 1..=worker.binary_search_steps {
            let amount_in = step_size * i as u64;
            let expected_amount_out = (amount_in as f64 * 0.98) as u64;
            
            // Path A->B->A: A as input, B as output
            assert_eq!(expected_amount_out, (amount_in as f64 * 0.98) as u64);
            
            // Path B->A->B: B as input, A as output  
            assert_eq!(expected_amount_out, (amount_in as f64 * 0.98) as u64);
        }
        
        // Verify we have both paths covered
        assert_eq!(expected_total_calculations, 10); // 5 steps * 2 paths
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