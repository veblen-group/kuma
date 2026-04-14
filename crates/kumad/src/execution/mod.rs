//! Trade execution worker.
//!
//! Receives `CrossChainSingleHop` signals from all strategy workers and executes them as
//! sequential two-legged trades via the Tycho Router.
//!
//! ## One-at-a-time gate
//!
//! Only one trade runs at a time. If a new signal arrives while a trade is in-flight it is
//! **dropped** — the in-flight trade takes priority. This keeps nonce management simple and
//! avoids competing against our own transactions in the same pool.
//!
//! ## Trade promotion
//!
//! `signal.try_promote()` encodes both transaction legs and returns a `Trade`. The `Trade`
//! then runs sequentially: slow chain transaction submitted and confirmed first, fast chain
//! second. This minimises settlement risk from mismatched block times.
//!
//! ## DB writes
//!
//! Trade results (successful, failed-slow, failed-fast) are written to the database
//! asynchronously via `FuturesUnordered` — failures are logged but never fatal.

use std::{collections::HashMap, pin::Pin};

use color_eyre::eyre::{self, WrapErr as _};
use futures::{
    Future, FutureExt,
    future::{Fuse, FusedFuture as _},
    pin_mut,
    stream::{FuturesUnordered, StreamExt, select_all},
};
use tokio::{select, sync::mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument};

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
        HashMap<strategy::CrossChainSingleHop, mpsc::Receiver<signals::CrossChainSingleHop>>,
    shutdown_token: CancellationToken,
    #[allow(dead_code)]
    db: database::Handle,
}

impl Worker {
    #[instrument(name = "trade_execution_worker", skip(self))]
    pub async fn run(self) -> eyre::Result<()> {
        info!("Starting trade execution system",);

        // Create a stream of signal receivers that maintain connections
        let broadcasts = self
            .signal_rxs
            // Map the HashMap values (the receivers) into streams
            .into_values()
            .map(ReceiverStream::new)
            .collect::<FuturesUnordered<_>>();
        let mut signal_stream = select_all(broadcasts);

        let mut db_writes: FuturesUnordered<
            Pin<Box<dyn Future<Output = eyre::Result<()>> + Send>>,
        > = FuturesUnordered::new();

        let curr_trade = Fuse::terminated();
        pin_mut!(curr_trade);

        loop {
            select! {
                biased;

                () = self.shutdown_token.cancelled() => {
                    info!("Trade execution worker received shutdown signal");
                    break Ok(());
                }

                // Prioritize handling in-flight trade results to free up for next signal
                trade_result = &mut curr_trade, if !curr_trade.is_terminated() => {
                    debug!("Trade execution worker received trade result");
                    let trade_result = match trade_result {
                        Ok(trade_result) => trade_result,
                        Err(err) => {
                            error!(%err, "Failed to receive trade result");
                            continue;
                        }
                    };

                    let repo = self.db.trade_repository();
                    db_writes.push(
                        async move {
                            repo.insert_trade_result(trade_result)
                                .await
                                .wrap_err("failed to write trade result to database")
                        }.boxed()
                    );
                }

                // If no running trade, process next generated signal
                Some(signal) = signal_stream.next(), if curr_trade.is_terminated() => {
                    info!(
                        %signal,
                        "💰 Received trade signal. executing cross-chain arbitrage",
                    );

                    let trade = match signal.try_promote() {
                        Ok(trade) => trade,
                        Err(err) => {
                            error!(%err, "Failed to convert signal into trade");
                            continue;
                        }
                    };

                    curr_trade.set(trade.run().fuse());
                },

                Some(res) = db_writes.next() => {
                    if let Err(e) = res {
                        error!("DB insert failed: {:?}", e);
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
