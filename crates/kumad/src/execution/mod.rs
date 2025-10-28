//! Trade execution module for executing cross-chain arbitrage trades

use std::{collections::HashMap, pin::Pin};

use color_eyre::eyre::{self, WrapErr as _};
use futures::{
    Future,
    stream::{FuturesUnordered, StreamExt},
};
use tokio::{select, sync::broadcast};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument};

use kuma_core::{database, signals, strategy};

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
    signal_rxs:
        HashMap<strategy::CrossChainSingleHop, broadcast::Receiver<signals::CrossChainSingleHop>>,
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
        for (strategy, mut rx) in self.signal_rxs.into_iter() {
            signal_stream.push(async move {
                rx.recv()
                    .await
                    .map(|signal| (strategy, signal))
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
                    let Ok((strategy, signal)) = result else {
                        error!("Failed to receive signal from channel");
                        continue;
                    };

                    info!(
                        // TODO: display funcs for signal and strategy
                        strategy.token_a = %strategy.token_a_symbol(),
                        strategy.token_b = %strategy.token_b_symbol(),
                        strategy.slow_chain = %signal.slow_chain.name,
                        strategy.fast_chain = %signal.fast_chain.name,
                        signal.slow_height = signal.slow_height,
                        signal.fast_height = signal.fast_height,
                        signal.expected_profit = %signal.expected_profit.0,
                        signal.expected_profit_b = %signal.expected_profit.1,
                        "💰 Received trade signal. executing cross-chain arbitrage",
                    );

                    // TODO: rename this to .try_promote()
                    let Ok(trade) = signal.try_into_trade() else {
                        error!("Failed to convert signal into trade");
                        continue;
                    };

                    // TODO: fused future
                    // TODO: rename this to .run()
                    let Ok(receipts) = trade.promote().await else {
                        error!("Failed to promote trade");
                        continue;
                    };

                    // TODO: pretty print these
                    info!(?receipts, ?strategy, "✅ Successfully executed cross-chain arbitrage trade for strategy");
                }

                else => {
                    info!("All strategy signal channels closed, shutting down trade execution worker");
                    break Ok(());
                }
            }
        }
    }
}
