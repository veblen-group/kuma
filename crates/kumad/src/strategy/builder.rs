use std::time::Duration;

use color_eyre::eyre::{self};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use kuma_core::{database, signals, state::block::BlockStateStream, strategy};

use super::{Handle, Worker};

pub struct Builder {
    pub strategy_config: strategy::CrossChainSingleHop,
    pub slow_stream: BlockStateStream,
    pub fast_stream: BlockStateStream,
    pub slow_block_time: Duration,
    pub db: database::Handle,
}

impl Builder {
    #[allow(clippy::type_complexity)]
    pub fn build(
        self,
    ) -> eyre::Result<(
        Handle,
        mpsc::Receiver<(signals::CrossChainSingleHop, oneshot::Receiver<i64>)>,
    )> {
        let Self {
            strategy_config: strategy,
            slow_stream,
            fast_stream,
            slow_block_time: slow_block_time_ms,
            db,
        } = self;

        // Create broadcast channel for signals
        let (signal_tx, signal_rx) =
            mpsc::channel::<(signals::CrossChainSingleHop, oneshot::Receiver<i64>)>(256);

        let shutdown_token = CancellationToken::new();

        let worker = Worker {
            strategy: strategy.clone(),
            slow_stream,
            fast_stream,
            signal_tx,
            shutdown_token: shutdown_token.clone(),
            slow_block_time: slow_block_time_ms,
            db,
        };

        let worker_handle = tokio::task::spawn(async move { worker.run().await });

        Ok((
            Handle {
                strategy: strategy.clone(),
                shutdown_token,
                worker_handle: Some(worker_handle),
            },
            signal_rx,
        ))
    }
}
