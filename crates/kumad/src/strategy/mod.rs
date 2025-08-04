//! Strategy module for managing cross-chain arbitrage signal generation
//!
//! This module implements the Builder-Handle-Worker pattern for strategy execution.
//! The strategy reads from two different blockchain networks (slow and fast) and
//! generates trading signals based on cross-chain arbitrage opportunities.

use std::{pin::Pin, time::Duration};

use color_eyre::eyre::{self, WrapErr as _};
use futures::Future;
use tokio::{select, sync::broadcast, time::Instant};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, trace};

use kuma_core::{
    signals,
    state::pair::PairStateStream,
    strategy::{self, Precomputes},
};

pub use builder::Builder;
mod builder;

pub struct Handle {
    shutdown_token: CancellationToken,
    worker_handle: Option<tokio::task::JoinHandle<eyre::Result<()>>>,
    #[allow(dead_code)]
    signal_rx: broadcast::Receiver<signals::CrossChainSingleHop>,
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

    #[allow(dead_code)]
    pub fn get_signal_rx(&self) -> broadcast::Receiver<signals::CrossChainSingleHop> {
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
    strategy: strategy::CrossChainSingleHop,
    slow_stream: PairStateStream,
    fast_stream: PairStateStream,
    signal_tx: broadcast::Sender<signals::CrossChainSingleHop>,
    shutdown_token: CancellationToken,
    slow_block_time: Duration,
}

impl Worker {
    #[instrument(name = "strategy_worker", skip(self))]
    pub async fn run(mut self) -> eyre::Result<()> {
        info!("Starting strategy worker");

        let submission_delay = self.slow_block_time.mul_f64(0.75);
        let mut submission_deadline = None;
        let mut precompute: Option<Precomputes> = None;
        let mut curr_signal = None;

        // biased loop
        // 1. shutdown signal
        // 2. timer ended and there's a signal to emit - populate the signal emission
        // 2. slow chain updates
        //  1. set up signal generation timer
        //  2. precompute
        // 3. fast chain updates
        //  1. try to generate signal from precompute
        //  2. overwrite current signal
        // 4. db write
        // 5. emit signal

        loop {
            select! {
                biased;

                () = self.shutdown_token.cancelled() => {
                    info!("Strategy worker received shutdown signal");
                    break Ok(());
                }

                // emit signal when timer ends if one exists
                _ = async {
                    if let Some(deadline) = submission_deadline {
                        tokio::time::sleep_until(deadline).await
                    } else {
                        futures::future::pending().await
                    }
                }, if curr_signal.is_some() => {
                    let signal = curr_signal.take().expect("Signal checked to be Some");
                    debug!(%signal, "📡 Emitting signal");

                    let signal_tx = self.signal_tx.clone();
                    signal_tx.send(signal).wrap_err("Signal sent")?;
                }

                // Handle slow chain updates
                Some(slow_state) = self.slow_stream.next() => {
                    let new_precompute = self.strategy.precompute(slow_state);

                    debug!(
                        block.height = new_precompute.block_height,
                        "✅ Precomputed trade sizes for slow chain"
                    );
                    precompute = Some(new_precompute);

                    // TODO: db write the spot prices & block update
                    // db.write(precompute.spot_prices[0])
                    // db.write(precompute.spot_prices[len-1])

                    // Start timer for 75% of block time
                    submission_deadline = Some(Instant::now() + submission_delay);

                    // TODO: clean up log
                    debug!(
                        ?submission_deadline,
                        "⏰ Started timer for next signal generation"
                    );
                }

                // TODO: handle for processing fast blocks
                // 1. update the fast current block
                // 2. write to db
                // 3. log a trace

                // Handle timer expiration for signal generation
                Some(fast_state) = self.fast_stream.next() => {
                    if let Some(precompute) = precompute.as_ref() {
                        // Step 3: Read latest fast chain state and generate signal
                        // TODO: fix this to use the curr fast state object
                        let (slow_height, fast_height) = (precompute.block_height, fast_state.block_height);

                        match self.strategy.generate_signal(precompute, fast_state) {
                            Ok(signal) => {
                                info!(
                                    %signal,
                                    "📡 Generated cross-chain signal"
                                );
                                curr_signal = Some(signal);
                                panic!("Signal generated")
                            }
                            Err(e) => {
                                debug!(
                                    %slow_height,
                                    %fast_height,
                                    error = %e,
                                    "No signal found for given blocks"
                                );
                            }
                        }
                    } else {
                        trace!(block.height = fast_state.block_height, "New fast chain state but no slow chain precompute, skipping signal generation");
                    }
                }
            }
        }
    }
}
