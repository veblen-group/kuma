use color_eyre::eyre::{self};
use num_bigint::BigUint;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use kuma_core::{
    chain::Chain,
    signals::CrossChainSingleHop,
    state::pair::{Pair, PairStateStream},
    strategy,
};

use super::{Handle, Worker};

pub struct Builder {
    pub slow_pair: Pair,
    pub slow_chain: Chain,
    pub fast_pair: Pair,
    pub fast_chain: Chain,
    pub slow_inventory: (BigUint, BigUint),
    pub fast_inventory: (BigUint, BigUint),
    pub binary_search_steps: usize,
    pub max_slippage_bps: u64,
    pub congestion_risk_discount_bps: u64,
    pub(crate) slow_stream: PairStateStream,
    pub(crate) fast_stream: PairStateStream,
    pub(crate) slow_block_time_ms: u64,
    pub(crate) signal_buffer_size: usize,
}

impl Builder {
    pub fn build(self) -> eyre::Result<Handle> {
        let Self {
            slow_stream,
            fast_stream,
            slow_block_time_ms,
            signal_buffer_size,
            slow_pair,
            slow_chain,
            fast_pair,
            fast_chain,
            slow_inventory,
            fast_inventory,
            binary_search_steps,
            max_slippage_bps,
            congestion_risk_discount_bps,
        } = self;

        // Create broadcast channel for signals
        let (signal_tx, signal_rx) = broadcast::channel::<CrossChainSingleHop>(signal_buffer_size);

        let shutdown_token = CancellationToken::new();

        let strategy = strategy::CrossChainSingleHop {
            slow_pair,
            slow_chain,
            fast_pair,
            fast_chain,
            slow_inventory,
            fast_inventory,
            binary_search_steps,
            max_slippage_bps,
            congestion_risk_discount_bps,
        };

        let worker = Worker {
            strategy,
            slow_stream,
            fast_stream,
            signal_tx,
            shutdown_token: shutdown_token.clone(),
            slow_block_time_ms,
        };

        let worker_handle = tokio::task::spawn(async move { worker.run().await });

        Ok(Handle {
            shutdown_token,
            worker_handle: Some(worker_handle),
            signal_rx,
        })
    }
}
