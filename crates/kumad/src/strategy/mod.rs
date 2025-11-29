//! Strategy worker for cross-chain arbitrage signal generation and persistence.
//!
//! This module coordinates slow and fast chain state streams to generate profitable arbitrage signals.
//! It employs a timing-based submission system, triggered at 75% of the slow block time, to maximize signal freshness.
//! All generated spot prices and arbitrage signals are persisted to the database for analysis and monitoring.

use std::{pin::Pin, time::Duration};

use color_eyre::eyre::{self, WrapErr as _, eyre};
use futures::{Future, FutureExt as _, stream::FuturesUnordered};
use tokio::{select, sync::broadcast, time::Instant};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument};

use kuma_core::{
    chain::Chain,
    database, signals,
    spot_prices::SpotPrices,
    state::{
        block::BlockStateStream,
        pair::{Pair, PairState},
    },
    strategy::{self, Precomputes, simulation::make_sorted_spot_prices},
};

pub use builder::Builder;
mod builder;

pub struct Handle {
    shutdown_token: CancellationToken,
    worker_handle: Option<tokio::task::JoinHandle<eyre::Result<()>>>,
    strategy: strategy::CrossChainSingleHop,
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

    pub fn get_signal_rx(&self) -> broadcast::Receiver<signals::CrossChainSingleHop> {
        self.signal_rx.resubscribe()
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
    signal_tx: broadcast::Sender<signals::CrossChainSingleHop>,
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
        let mut precompute: Option<(Precomputes, SpotPrices, SpotPrices)> = None;
        let mut curr_signal = None;
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
                    let signal = curr_signal.take().expect("Signal checked to be Some");
                    debug!(%signal, "📡 Emitting signal");

                    self.signal_tx.send(signal).wrap_err("Signal sent")?;
                }

                // Handle slow chain updates
                Some(slow_state) = self.slow_stream.next() => {
                    // Start timer for 75% of block time
                    submission_deadline = Some(Instant::now() + submission_delay);

                    debug!(
                        block.height = slow_state.pair_state.block_height,
                        "⏱️ Slow block received. Started timer for next signal generation."
                    );

                    // Generate precomputes
                    let new_precompute = self.strategy.precompute(slow_state.pair_state);
                    debug!(
                        block.height = new_precompute.block_height,
                        "✅ Precomputed trade sizes for slow chain"
                    );

                    let prices_a_b = SpotPrices::from_precompute(
                        &new_precompute,
                        self.strategy.slow_chain.clone(),
                        self.strategy.slow_pair.clone()
                    );

                    // calculate usdc spot prices
                    // TODO: parallelize with the a<->b simulation
                    let prices_a_usdc = make_prices(&slow_state.token_a_usdc_state, self.strategy.slow_token_a_usdc.clone(), self.strategy.slow_chain.clone())
                        .wrap_err_with(|| format!("failed to simulate spot prices for {}", self.strategy.slow_token_a_usdc))?;
                    let prices_b_usdc = make_prices(&slow_state.token_b_usdc_state, self.strategy.slow_token_b_usdc.clone(), self.strategy.slow_chain.clone())
                        .wrap_err_with(|| format!("failed to simulate spot prices for {}", self.strategy.slow_token_b_usdc))?;

                    let repo = self.db.spot_price_repository();
                    db_writes.push({
                        let pair_a_b = self.strategy.slow_pair.clone();
                        let pair_a_usdc = self.strategy.slow_token_a_usdc.clone();
                        let pair_b_usdc = self.strategy.slow_token_b_usdc.clone();

                        let prices_a_b = prices_a_b.clone();
                        let prices_a_usdc = prices_a_usdc.clone();
                        let prices_b_usdc = prices_b_usdc.clone();

                        async move {
                            repo.insert(prices_a_b).await.wrap_err_with(|| format!("failed to write spot prices to db for {}", pair_a_b))?;
                            repo.insert(prices_a_usdc).await.wrap_err_with(|| eyre!("failed to write spot prices to db for {}", pair_a_usdc))?;
                            repo.insert(prices_b_usdc).await.wrap_err_with(|| eyre!("failed to write spot prices to db for {}", pair_b_usdc))?;
                            Ok(())
                        }
                    }.boxed());

                    precompute = Some((new_precompute, prices_a_usdc, prices_b_usdc));
                }

                // Handle timer expiration for signal generation
                Some(fast_state) = self.fast_stream.next() => {
                    if let Some((precompute, slow_prices_a_usdc, slow_prices_b_usdc)) = precompute.as_ref() {
                        // Step 3: Read latest fast chain state and generate signal
                        let (slow_height, fast_height) = (precompute.block_height, fast_state.pair_state.block_height);
                        let fast_sorted_spot_prices = make_sorted_spot_prices(&fast_state.pair_state, &self.strategy.fast_pair);

                        let prices_a_b = SpotPrices::try_from_sorted_prices(
                            &fast_sorted_spot_prices,
                            fast_state.pair_state.block_height,
                            self.strategy.fast_chain.clone(),
                            self.strategy.fast_pair.clone()
                        ).wrap_err_with(|| format!("fast chain spot prices should exist at height {}", fast_height))?;

                        // calculate usdc spot prices
                        let fast_prices_a_usdc = make_prices(&fast_state.token_a_usdc_state, self.strategy.fast_token_a_usdc.clone(),self.strategy.fast_chain.clone())
                            .wrap_err_with(|| format!("failed to write spot prices to db for {}", self.strategy.fast_token_a_usdc))?;
                        let fast_prices_b_usdc = make_prices(&fast_state.token_b_usdc_state, self.strategy.fast_token_b_usdc.clone(), self.strategy.fast_chain.clone())
                            .wrap_err_with(|| format!("failed to write spot prices to db for {}", self.strategy.fast_token_b_usdc))?;

                        let repo = self.db.spot_price_repository();
                        db_writes.push({
                            let pair_a_b = self.strategy.fast_pair.clone();
                            let pair_a_usdc = self.strategy.fast_token_a_usdc.clone();
                            let pair_b_usdc = self.strategy.fast_token_b_usdc.clone();

                            let fast_prices_a_usdc = fast_prices_a_usdc.clone();
                            let fast_prices_b_usdc = fast_prices_b_usdc.clone();

                            async move {
                                repo.insert(prices_a_b).await.wrap_err_with(|| format!("failed to write spot prices to db for {}", pair_a_b))?;
                                repo.insert(fast_prices_a_usdc).await.wrap_err_with(|| eyre!("failed to write spot prices to db for {}", pair_a_usdc))?;
                                repo.insert(fast_prices_b_usdc).await.wrap_err_with(|| eyre!("failed to write spot prices to db for {}", pair_b_usdc))?;
                                Ok(())
                            }
                        }.boxed());


                        match self.strategy.generate_signal(precompute, slow_prices_a_usdc, slow_prices_b_usdc, fast_state.pair_state, fast_sorted_spot_prices, &fast_prices_a_usdc, &fast_prices_b_usdc) {
                            Ok(signal) => {
                                info!(
                                    %signal,
                                    "📡 Generated cross-chain signal"
                                );

                                let json_signal = serde_json::to_string(&signal).wrap_err("failed to serialize signal to json")?;
                                info!(%json_signal, "Serialized signal to json");

                                curr_signal = Some(signal.clone());

                                // Save generated signal to db and update it for emission
                                let repo = self.db.signal_repository();
                                db_writes.push(async move {
                                    repo.insert(signal.clone()).await.map_err(|e| {
                                        eyre!("failed to write signal to db: {e:}")
                                    })
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

fn make_prices(state: &PairState, pair: Pair, chain: Chain) -> eyre::Result<SpotPrices> {
    let sorted_spot_prices = make_sorted_spot_prices(state, &pair);

    let spot_prices = SpotPrices::try_from_sorted_prices(
        &sorted_spot_prices,
        state.block_height,
        chain,
        pair.clone(),
    )?;

    debug!(
        block.height = spot_prices.block_height,
        %pair,
        "✅ Generated USDC spot prices for token A"
    );

    Ok(spot_prices)
}
