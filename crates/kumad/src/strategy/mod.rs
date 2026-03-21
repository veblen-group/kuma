//! Strategy worker for cross-chain arbitrage signal generation and persistence.
//!
//! This module coordinates slow and fast chain state streams to generate profitable arbitrage signals.
//! It employs a timing-based submission system, triggered at 75% of the slow block time, to maximize signal freshness.
//! All generated spot prices and arbitrage signals are persisted to the database for analysis and monitoring.

use std::{pin::Pin, time::Duration};

use color_eyre::eyre::{self, WrapErr as _, eyre};
use futures::{Future, FutureExt as _, stream::FuturesUnordered};
use tokio::{
    select,
    sync::{mpsc, oneshot},
    time::Instant,
};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument};

use kuma_core::{
    database::{self, SignalRepository, SpotPriceRepository},
    signals,
    spot_prices::{SpotPrices, try_make_sorted_spot_prices},
    state::block::BlockStateStream,
    strategy::{self, Precomputes},
};



pub use builder::Builder;
mod builder;

pub struct Handle {
    shutdown_token: CancellationToken,
    worker_handle: Option<tokio::task::JoinHandle<eyre::Result<()>>>,
    strategy: strategy::CrossChainSingleHop,
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

    pub fn strategy_config(&self) -> strategy::CrossChainSingleHop {
        self.strategy.clone()
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
    strategy: strategy::CrossChainSingleHop,
    slow_stream: BlockStateStream,
    fast_stream: BlockStateStream,
    signal_tx: mpsc::Sender<(signals::CrossChainSingleHop, oneshot::Receiver<i64>)>,
    shutdown_token: CancellationToken,
    slow_block_time: Duration,
    db: database::Handle,
}

// ---------------------------------------------------------------------------
// Static DB write functions — no &self borrow, all deps passed explicitly.
// This avoids borrow conflicts with the select loop's &mut self arms.
// ---------------------------------------------------------------------------

/// Insert slow-chain spot prices into the database — fire and forget.
/// Spot price FK linkage is resolved at signal insert time via a SQL CTE.
async fn write_slow_spot_prices(
    repo: SpotPriceRepository,
    prices_a_b: SpotPrices,
    prices_a_usdc: Option<SpotPrices>,
    prices_b_usdc: Option<SpotPrices>,
    prices_eth_usdc: Option<SpotPrices>,
) -> eyre::Result<()> {
    let pair = prices_a_b.pair.clone();
    repo.insert(prices_a_b)
        .await
        .wrap_err_with(|| format!("failed to write spot prices to db for {pair}"))?;

    if let Some(prices) = prices_a_usdc {
        let pair = prices.pair.clone();
        repo.insert(prices)
            .await
            .wrap_err_with(|| eyre!("failed to write spot prices to db for {pair}"))?;
    }

    if let Some(prices) = prices_b_usdc {
        let pair = prices.pair.clone();
        repo.insert(prices)
            .await
            .wrap_err_with(|| eyre!("failed to write spot prices to db for {pair}"))?;
    }

    if let Some(prices) = prices_eth_usdc {
        let pair = prices.pair.clone();
        repo.insert(prices)
            .await
            .wrap_err_with(|| eyre!("failed to write ETH-USDC spot prices to db for {pair}"))?;
    }

    Ok(())
}

/// Insert fast-chain A/B spot prices — fire and forget.
async fn write_fast_spot_prices(
    repo: SpotPriceRepository,
    prices_a_b: SpotPrices,
) -> eyre::Result<()> {
    let pair = prices_a_b.pair.clone();
    repo.insert(prices_a_b)
        .await
        .wrap_err_with(|| format!("failed to write spot prices to db for {pair}"))?;
    Ok(())
}

/// Insert a signal. Spot price FKs are resolved inside the INSERT via a CTE
/// that looks up `spot_prices` rows by (chain, pair, block_height) — no
/// cross-future coordination needed.
async fn write_signal(
    repo: SignalRepository,
    signal: signals::CrossChainSingleHop,
    signal_id_tx: oneshot::Sender<i64>,
) -> eyre::Result<()> {
    let id = repo
        .insert(signal)
        .await
        .map_err(|e| eyre!("failed to write signal to db: {e:}"))?;

    signal_id_tx
        .send(id)
        .map_err(|_| eyre!("failed to send signal id to channel"))?;

    Ok(())
}

impl Worker {
    /// The main event loop for the strategy worker. It uses a `tokio::select!` biased loop
    /// to prioritize shutdown signals, then signal emission, and then process slow and fast
    /// chain updates. Database writes are handled concurrently via `FuturesUnordered`.
    #[instrument(name = "strategy_worker", skip(self), fields(
        slow_chain = self.strategy.slow_chain.name.to_string(),
        fast_chain = self.strategy.fast_chain.name.to_string(),
        token_a = self.strategy.slow_pair.token_a().symbol,
        token_b = self.strategy.slow_pair.token_b().symbol))]
    pub async fn run(mut self) -> eyre::Result<()> {
        info!("Starting strategy worker");

        let submission_delay = self.slow_block_time.mul_f64(0.75);
        let mut submission_deadline = None;

        let mut precompute: Option<Precomputes> = None;
        let mut curr_signal: Option<(signals::CrossChainSingleHop, oneshot::Receiver<i64>)> = None;

        let mut db_writes: FuturesUnordered<
            Pin<Box<dyn Future<Output = eyre::Result<()>> + Send>>,
        > = FuturesUnordered::new();

        loop {
            select! {
                biased;

                () = self.shutdown_token.cancelled() => {
                    info!("Strategy worker received shutdown signal");
                    break Ok(());
                }

                // Emit signal when timer ends if one exists
                _ = async {
                    if let Some(deadline) = submission_deadline {
                        tokio::time::sleep_until(deadline).await
                    } else {
                        futures::future::pending().await
                    }
                }, if curr_signal.is_some() => {
                    let (signal, id_rx) = curr_signal.take().expect("Signal checked to be Some");
                    debug!(%signal, "📡 Emitting signal");
                    self.signal_tx.send((signal, id_rx)).await
                        .wrap_err("failed to send signal to emitter")?;
                }

                // Handle slow chain updates
                Some(slow_state) = self.slow_stream.next() => {
                    submission_deadline = Some(Instant::now() + submission_delay);
                    debug!(
                        block.height = slow_state.pair_state.block_height,
                        "⏱️ Slow block received. Started timer for next signal generation."
                    );

                    let new_precompute = self.strategy.try_precompute(slow_state, None)?;
                    debug!(
                        block.height = new_precompute.block_height,
                        "✅ Precomputed trade sizes for slow chain"
                    );

                    db_writes.push(write_slow_spot_prices(
                        self.db.spot_price_repository(),
                        new_precompute.prices_a_b.clone(),
                        new_precompute.prices_a_usdc.clone(),
                        new_precompute.prices_b_usdc.clone(),
                        new_precompute.prices_eth_usdc.clone(),
                    ).boxed());

                    precompute = Some(new_precompute);
                }

                // Handle fast chain updates
                Some(fast_state) = self.fast_stream.next() => {
                    let sorted_prices_a_b = try_make_sorted_spot_prices(&fast_state.pair_state, &self.strategy.fast_pair)
                        .wrap_err_with(|| format!("failed to simulate spot prices for {} on {}", self.strategy.fast_pair, self.strategy.fast_chain))?;
                    let prices_a_b = SpotPrices::try_from_sorted_prices(
                        &sorted_prices_a_b,
                        fast_state.pair_state.block_height,
                        self.strategy.fast_chain.clone(),
                        self.strategy.fast_pair.clone(),
                    )?;

                    if let Some(precompute) = precompute.as_ref() {
                        let (slow_height, fast_height) = (precompute.block_height, fast_state.pair_state.block_height);

                        match self.strategy.generate_signal(precompute, fast_state.pair_state, sorted_prices_a_b, fast_state.base_fee) {
                            Ok(signal) => {
                                info!(%signal, "📡 Generated cross-chain signal");

                                db_writes.push(write_fast_spot_prices(
                                    self.db.spot_price_repository(),
                                    prices_a_b,
                                ).boxed());

                                let (signal_id_tx, signal_id_rx) = oneshot::channel();
                                curr_signal = Some((signal.clone(), signal_id_rx));

                                db_writes.push(write_signal(
                                    self.db.signal_repository(),
                                    signal,
                                    signal_id_tx,
                                ).boxed());
                            }
                            Err(e) => {
                                debug!(
                                    %slow_height,
                                    %fast_height,
                                    error = %e,
                                    "No signal found for given blocks"
                                );

                                db_writes.push(write_fast_spot_prices(
                                    self.db.spot_price_repository(),
                                    prices_a_b,
                                ).boxed());
                            }
                        }
                    } else {
                        debug!(
                            block.height = fast_state.pair_state.block_height,
                            "New fast chain state but no slow chain precompute, skipping signal generation"
                        );

                        db_writes.push(write_fast_spot_prices(
                            self.db.spot_price_repository(),
                            prices_a_b,
                        ).boxed());
                    }
                }

                Some(res) = db_writes.next() => {
                    if let Err(e) = res {
                        error!("DB insert failed: {:?}", e);
                    }
                }
            }
        }
    }
}
