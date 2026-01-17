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
    database, signals,
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

impl Worker {
    /// The main event loop for the strategy worker. It uses a `tokio::select!` biased loop
    /// to prioritize shutdown signals, then signal emission, and then process slow and fast
    /// chain updates. Database writes are handled concurrently via `FuturesUnordered`.
    ///
    /// The loop operates as follows:
    ///
    /// 1.  **Shutdown Signal**: Always the highest priority. If a shutdown signal is received,
    ///     the worker will gracefully exit.
    /// 2.  **Signal Emission Timer**: If a signal is present (`curr_signal` is `Some`) and the
    ///     `submission_deadline` has been reached, the signal is emitted via `signal_tx`.
    /// 3.  **Slow Chain Updates**: When a new slow chain block state is received:
    ///     *   A timer (`submission_deadline`) is started for emitting the signal (75% of the slow block time).
    ///     *   Precomputations are performed based on the slow chain state.
    ///     *   Spot prices for the slow chain (A/B, A/USDC, B/USDC) are calculated and pushed
    ///         to `db_writes` for asynchronous persistence.
    /// 4.  **Fast Chain Updates**: When a new fast chain block state is received:
    ///     *   If `precompute` data from the slow chain is available, a cross-chain signal is
    ///         attempted to be generated using the latest fast chain state and the slow chain precomputes.
    ///     *   Spot prices for the fast chain (A/B, A/USDC, B/USDC) are calculated and pushed
    ///         to `db_writes` for asynchronous persistence.
    ///     *   If a profitable signal is generated, it updates `curr_signal` and is also
    ///         pushed to `db_writes` for asynchronous persistence.
    /// 5.  **Database Writes**: Any completed database write futures from `db_writes` are
    ///     polled. Errors during database writes are logged but do not block the main loop.
    #[instrument(name = "strategy_worker", skip(self), fields(
        slow_chain = self.strategy.slow_chain.name.to_string(),
        fast_chain = self.strategy.fast_chain.name.to_string(),
        token_a = self.strategy.slow_pair.token_a().symbol,
        token_b = self.strategy.slow_pair.token_b().symbol))]
    pub async fn run(mut self) -> eyre::Result<()> {
        info!("Starting strategy worker");

        // only submit late into the slow block
        let submission_delay = self.slow_block_time.mul_f64(0.75);
        let mut submission_deadline = None;

        // TODO: curr eth_usdc price
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

                // emit signal when timer ends if one exists
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

                // TODO: handle eth usdc price updates

                // Handle slow chain updates
                Some(slow_state) = self.slow_stream.next() => {
                    // Start timer for 75% of block time
                    // TODO: add this deadline to the signal metadata?
                    submission_deadline = Some(Instant::now() + submission_delay);

                    debug!(
                        block.height = slow_state.pair_state.block_height,
                        "⏱️ Slow block received. Started timer for next signal generation."
                    );

                    // Generate precomputes
                    // TODO: reuse unmodified precomputes if possible
                    // TODO: require curr eth_usdc price and use basefee from slowstate
                    let new_precompute = self.strategy.try_precompute(slow_state, None)?;
                    debug!(
                        block.height = new_precompute.block_height,
                        "✅ Precomputed trade sizes for slow chain"
                    );

                    let repo = self.db.spot_price_repository();
                    db_writes.push({

                        let pair_eth_usdc = self.strategy.slow_eth_usdc.clone();
                        let pair_a_b = self.strategy.slow_pair.clone();
                        let pair_a_usdc = self.strategy.slow_token_a_usdc.clone();
                        let pair_b_usdc = self.strategy.slow_token_b_usdc.clone();

                        let prices_eth_usdc = new_precompute.prices_eth_usdc.clone();
                        let prices_a_b = new_precompute.prices_a_b.clone();
                        let prices_a_usdc = new_precompute.prices_a_usdc.clone();
                        let prices_b_usdc = new_precompute.prices_b_usdc.clone();

                        async move {
                            info!(
                                chain = %self.strategy.slow_chain.name,
                                block_height = %new_precompute.block_height,
                                pair = %pair_a_b,
                                prices = %prices_a_b,
                                "📈 Saving spot prices to database"
                            );
                            repo.insert(prices_a_b).await.wrap_err_with(|| format!("failed to write spot prices to db for {}", pair_a_b))?;

                            if let (Some(pair_a_usdc), Some(prices_a_usdc)) = (pair_a_usdc, prices_a_usdc) {
                                info!(
                                    chain = %self.strategy.slow_chain.name,
                                    block_height = %new_precompute.block_height,
                                    pair = %pair_a_usdc,
                                    prices = %prices_a_usdc,
                                    "📈 Saving spot prices to database"
                                );
                                repo.insert(prices_a_usdc).await.wrap_err_with(|| eyre!("failed to write spot prices to db for {}", pair_a_usdc))?;
                            }

                            if let (Some(pair_b_usdc), Some(prices_b_usdc)) = (pair_b_usdc, prices_b_usdc) {
                                info!(
                                    chain = %self.strategy.slow_chain.name,
                                    block_height = %new_precompute.block_height,
                                    pair = %pair_b_usdc,
                                    prices = %prices_b_usdc,
                                    "📈 Saving spot prices to database"
                                );
                                repo.insert(prices_b_usdc).await.wrap_err_with(|| eyre!("failed to write spot prices to db for {}", pair_b_usdc))?;
                            }

                            if let (Some(pair_eth_usdc), Some(prices_eth_usdc)) = (pair_eth_usdc, prices_eth_usdc) {
                                info!(
                                    chain = %self.strategy.slow_chain.name,
                                    block_height = %new_precompute.block_height,
                                    pair = %pair_eth_usdc,
                                    prices = %prices_eth_usdc,
                                    "📈 Saving ETH-USDC spot prices to database"
                                );
                                repo.insert(prices_eth_usdc).await.wrap_err_with(|| eyre!("failed to write ETH-USDC spot prices to db for {}", pair_eth_usdc))?;
                            }
                            Ok(())
                        }
                    }.boxed());

                    precompute = Some(new_precompute);
                }

                // Handle timer expiration for signal generation
                Some(fast_state) = self.fast_stream.next() => {
                    // Step 3: Read latest fast chain state
                    let sorted_prices_a_b = try_make_sorted_spot_prices(&fast_state.pair_state, &self.strategy.fast_pair)
                        .wrap_err_with(|| format!("failed to simulate spot prices for {} on {}", self.strategy.fast_pair, self.strategy.fast_chain))?;
                    let prices_a_b = SpotPrices::try_from_sorted_prices(&sorted_prices_a_b, fast_state.pair_state.block_height, self.strategy.fast_chain.clone(), self.strategy.fast_pair.clone())?;


                    let repo = self.db.spot_price_repository();
                    db_writes.push({
                        let pair_a_b = self.strategy.fast_pair.clone();

                        async move {
                            info!(
                                chain = %self.strategy.fast_chain.name,
                                block_height = %fast_state.pair_state.block_height,
                                pair = %pair_a_b,
                                prices = %prices_a_b,
                                "📈 Saving spot prices to database"
                            );
                            repo.insert(prices_a_b).await.wrap_err_with(|| format!("failed to write spot prices to db for {}", pair_a_b))?;
                            Ok(())
                        }
                    }.boxed());

                    // TODO: require eth usdc price
                    // try to generate signal if precompute is available
                    if let Some(precompute) = precompute.as_ref() {
                        let (slow_height, fast_height) = (precompute.block_height, fast_state.pair_state.block_height);

                        // TODO: use eth_usdc price amd basefee from fast block
                        match self.strategy.generate_signal(precompute, fast_state.pair_state, sorted_prices_a_b) {
                            Ok(signal) => {
                                info!(
                                    %signal,
                                    "📡 Generated cross-chain signal"
                                );

                                let json_signal = serde_json::to_string(&signal).wrap_err("failed to serialize signal to json")?;
                                info!(%json_signal, "Serialized signal to json");

                                let (id_tx, id_rx) = oneshot::channel();
                                curr_signal = Some((signal.clone(), id_rx));

                                // Save generated signal to db and update it for emission
                                let repo = self.db.signal_repository();
                                db_writes.push(async move {
                                    let id = repo.insert(signal.clone()).await.map_err(|e| {
                                        eyre!("failed to write signal to db: {e:}")
                                    })?;

                                    id_tx.send(id).map_err(|e| {
                                        eyre!("failed to send signal id to channel: {e:}")
                                    })?;
                                    Ok(())
                                }.boxed());
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
                        debug!(
                            block.height = fast_state.pair_state.block_height,
                            "New fast chain state but no slow chain precompute, skipping signal generation"
                        );
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
