//! Strategy module for managing cross-chain arbitrage signal generation
//!
//! This module implements the Builder-Handle-Worker pattern for strategy execution.
//! The strategy reads from two different blockchain networks (slow and fast) and
//! generates trading signals based on cross-chain arbitrage opportunities.

use std::{pin::Pin, sync::Arc, time::Duration};

use color_eyre::eyre::{self, WrapErr as _};
use futures::Future;
use tokio::{select, sync::broadcast, time::Instant};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, trace, warn};

use kuma_core::{
    signals::CrossChainSingleHop,
    state::pair::{PairState, PairStateStream},
    strategy::CrossChainSingleHop as StrategyConfig,
};

pub use builder::Builder;
mod builder;

pub struct Handle {
    shutdown_token: CancellationToken,
    worker_handle: Option<tokio::task::JoinHandle<eyre::Result<()>>>,
    signal_rx: broadcast::Receiver<CrossChainSingleHop>,
}

impl Handle {
    pub async fn shutdown(&mut self) -> eyre::Result<()> {
        self.shutdown_token.cancel();
        if let Err(e) = self
            .worker_handle
            .take()
            .expect("shutdown must not be called twice")
            .await
        {
            error!("Strategy worker failed: {}", e);
            return Err(e.into());
        }
        Ok(())
    }

    pub fn get_signal_rx(&self) -> broadcast::Receiver<CrossChainSingleHop> {
        self.signal_rx.resubscribe()
    }
}

// Awaiting the handle deals with the Worker's result
impl Future for Handle {
    type Output = eyre::Result<()>;

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        use futures::future::FutureExt as _;

        let task = self
            .worker_handle
            .as_mut()
            .expect("strategy handle must not be polled after shutdown");

        task.poll_unpin(cx).map(|result| match result {
            Ok(worker_res) => match worker_res {
                Ok(()) => Ok(()),
                Err(e) => Err(e).wrap_err("strategy task returned with err"),
            },
            Err(e) => Err(e).wrap_err("strategy task panicked"),
        })
    }
}

struct Worker {
    // TODO: set up strategy object from core
    strategy_config: StrategyConfig,
    slow_stream: PairStateStream,
    fast_stream: PairStateStream,
    signal_tx: broadcast::Sender<CrossChainSingleHop>,
    shutdown_token: CancellationToken,
    slow_block_time_ms: u64,
}

impl Worker {
    #[instrument(name = "strategy_worker", skip(self))]
    pub async fn run(mut self) -> eyre::Result<()> {
        info!("Starting strategy worker");

        // TODO: use fusedfutures for signal emission and db write
        let mut slow_state: Option<PairState> = None;
        let mut precompute_result: Option<Arc<Vec<_>>> = None;
        let mut timer_deadline: Option<Instant> = None;

        // biased loop
        // 1. shutdown signal
        // 2. slow chain updates
        // 3. fast chain updates
        // 4. db write
        // 5. emit signal
        loop {
            select! {
                biased;

                () = self.shutdown_token.cancelled() => {
                    info!("Strategy worker received shutdown signal");
                    break Ok(());
                }

                // Handle slow chain updates
                Some(new_slow_state) = self.slow_stream.next() => {
                    // TODO: remove because its already logged in tycho collector
                    debug!(
                        block_height = new_slow_state.block_height,
                        "📊 Received slow chain state update"
                    );

                    // Step 1: Read slow chain state and precompute
                    // TODO: take ref
                    let precompute = self.strategy_config.precompute(&new_slow_state);

                    trace!(
                        precompute_count = precompute.len(),
                        "✅ Precomputed trade sizes for slow chain"
                    );
                    precompute_result = Some(Arc::new(precompute));
                    slow_state = Some(new_slow_state);

                    // TODO: db write the spot prices & block update

                    // Step 2: Start timer for 75% of block time
                    let delay_ms = (self.slow_block_time_ms as f64 * 0.75) as u64;
                    timer_deadline = Some(Instant::now() + Duration::from_millis(delay_ms));

                    trace!(
                        delay_ms = delay_ms,
                        "⏰ Started timer for next signal generation"
                    );
                }

                // TODO: handle for processing fast blocks
                // 1. update the fast current block
                // 2. write to db
                // 3. log a trace

                // Handle timer expiration for signal generation
                _ = async {
                    // TODO: this seems fucked
                    match timer_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => futures::future::pending().await,
                    }
                } => {
                    if let (Some(ref slow_state), Some(ref precompute)) = (&slow_state, &precompute_result) {
                        // Step 3: Read latest fast chain state and generate signal
                        // TODO: fix this to use the curr fast state object
                        if let Some(fast_state) = self.get_latest_fast_state().await {
                            info!(
                                slow_height = slow_state.block_height,
                                fast_height = fast_state.block_height,
                                "🚀 Generating signal from latest states"
                            );

                            match self.strategy_config.generate_signal(slow_state, &fast_state, precompute) {
                                Ok(signal) => {
                                    info!(
                                        slow_height = slow_state.block_height,
                                        fast_height = fast_state.block_height,
                                        "📡 Generated cross-chain signal"
                                    );

                                    // Step 4: Emit signal to broadcast channel
                                    if let Err(e) = self.signal_tx.send(signal) {
                                        warn!(error = %e, "No receivers for generated signal");
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        error = %e,
                                        slow_height = slow_state.block_height,
                                        fast_height = fast_state.block_height,
                                        "Failed to generate signal"
                                    );
                                }
                            }
                        } else {
                            warn!("No fast chain state available for signal generation");
                        }
                    }

                    // Reset timer
                    timer_deadline = None;
                }
            }
        }
    }
}
