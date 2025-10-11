//! Trade execution module for executing cross-chain arbitrage trades

use std::pin::Pin;

use color_eyre::eyre::{self, WrapErr as _};
use futures::Future;
use tokio::{select, sync::broadcast};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument, warn};

use kuma_core::{database, signals};

pub use builder::Builder;
mod builder;

pub struct Handle {
    shutdown_token: CancellationToken,
    worker_handle: Option<tokio::task::JoinHandle<eyre::Result<()>>>,
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
            error!("Trade execution worker failed: {}", e);
            return Err(e.into());
        }
        Ok(())
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
            .expect("trade execution handle must not be polled after shutdown");

        task.poll_unpin(cx).map(|result| match result {
            Ok(worker_res) => match worker_res {
                Ok(()) => Ok(()),
                Err(e) => Err(e).wrap_err("trade execution task returned with err"),
            },
            Err(e) => Err(e).wrap_err("trade execution task panicked"),
        })
    }
}

struct Worker {
    signal_rx: broadcast::Receiver<signals::CrossChainSingleHop>,
    shutdown_token: CancellationToken,
    #[allow(dead_code)]
    db: database::Handle,
}

impl Worker {
    #[instrument(name = "trade_execution_worker", skip(self))]
    pub async fn run(mut self) -> eyre::Result<()> {
        info!("Starting trade execution worker");

        loop {
            select! {
                biased;

                () = self.shutdown_token.cancelled() => {
                    info!("Trade execution worker received shutdown signal");
                    break Ok(());
                }

                signal = self.signal_rx.recv() => {
                    match signal {
                        Ok(signal) => {
                            info!(
                                %signal,
                                slow_chain = %signal.slow_chain,
                                fast_chain = %signal.fast_chain,
                                slow_height = signal.slow_height,
                                fast_height = signal.fast_height,
                                expected_profit_a = %signal.expected_profit.0,
                                expected_profit_b = %signal.expected_profit.1,
                                "💰 Received trade signal - would execute cross-chain arbitrage"
                            );
                            let trade = signal.try_into_trade()?;
                            let receipts = trade.promote().await?;
                            info!(?receipts, "✅ Successfully executed cross-chain arbitrage trade");
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped_signals = skipped, "Trade execution lagging behind signal generation");
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            info!("Signal channel closed, shutting down trade execution worker");
                            break Ok(());
                        }
                    }
                }
            }
        }
    }
}
