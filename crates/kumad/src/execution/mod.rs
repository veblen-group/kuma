//! Trade execution module for executing cross-chain arbitrage trades

use std::pin::Pin;

use color_eyre::eyre::{self, WrapErr as _};
use futures::{
    Future,
    stream::{FuturesUnordered, StreamExt},
};
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
    signal_rxs: Vec<broadcast::Receiver<signals::CrossChainSingleHop>>,
    shutdown_token: CancellationToken,
    #[allow(dead_code)]
    db: database::Handle,
}

impl Worker {
    #[instrument(name = "trade_execution_worker", skip(self))]
    pub async fn run(self) -> eyre::Result<()> {
        info!("Starting trade execution system",);

        // Create a stream of signal receivers that maintain connections
        let mut signal_stream = FuturesUnordered::new();
        for (idx, mut rx) in self.signal_rxs.into_iter().enumerate() {
            signal_stream.push(async move {
                rx.recv()
                    .await
                    .map(|signal| (idx, signal))
                    .wrap_err("Failed to receive signal")
            });
        }

        loop {
            select! {
                biased;

                () = self.shutdown_token.cancelled() => {
                    info!("Trade execution worker received shutdown signal");
                    break Ok(());
                }

                Some(result) = signal_stream.next() => {
                    match result {
                        Some((strategy_idx, signal)) => {
                            info!(
                                strategy_idx,
                                %signal,
                                slow_chain = %signal.slow_chain.name,
                                fast_chain = %signal.fast_chain.name,
                                slow_height = signal.slow_height,
                                fast_height = signal.fast_height,
                                expected_profit_a = %signal.expected_profit.0,
                                expected_profit_b = %signal.expected_profit.1,
                                "💰 Received trade signal from strategy {} - executing cross-chain arbitrage",
                                strategy_idx
                            );

                            // Execute the trade
                            match signal.try_into_trade() {
                                Ok(trade) => {
                                    match trade.promote().await {
                                        Ok(receipts) => {
                                            info!(?receipts, strategy_idx, "✅ Successfully executed cross-chain arbitrage trade for strategy {}", strategy_idx);
                                        }
                                        Err(e) => {
                                            error!("Failed to execute trade for strategy {}: {:?}", strategy_idx, e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to convert signal to trade for strategy {}: {:?}", strategy_idx, e);
                                }
                            }
                        }
                        None => {
                            info!("A strategy signal channel closed");
                            // Channel closed, but other channels may still be active
                        }
                    }
                }

                else => {
                    info!("All strategy signal channels closed, shutting down trade execution worker");
                    break Ok(());
                }
            }
        }
    }
}
