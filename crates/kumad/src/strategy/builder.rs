use std::sync::Arc;

use color_eyre::eyre::{self, Context as _};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use kuma_core::{
    signals::CrossChainSingleHop, state::pair::PairStateStream,
    strategy::CrossChainSingleHop as StrategyConfig,
};

use super::{Handle, Worker};

pub struct Builder {
    pub(crate) strategy_config: StrategyConfig,
    pub(crate) slow_stream: PairStateStream,
    pub(crate) fast_stream: PairStateStream,
    pub(crate) slow_block_time_ms: u64,
    pub(crate) signal_buffer_size: usize,
}

impl Builder {
    pub fn build(self) -> eyre::Result<Handle> {
        let Self {
            strategy_config,
            slow_stream,
            fast_stream,
            slow_block_time_ms,
            signal_buffer_size,
        } = self;

        // Create broadcast channel for signals
        let (signal_tx, signal_rx) = broadcast::channel::<CrossChainSingleHop>(signal_buffer_size);

        let shutdown_token = CancellationToken::new();

        // TODO: set up strategy object from core and pass it to worker
        let worker = Worker {
            strategy_config,
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
