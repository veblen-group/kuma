use alloy::{
    providers::{fillers::FillProvider, Provider, ProviderBuilder},
    transports::http::reqwest::Url,
};
use color_eyre::eyre::{self, Context as _};
use futures::{
    future::{Fuse, FusedFuture as _},
    FutureExt,
};
use futures::{Stream, StreamExt};
use std::{str::FromStr, sync::Arc};
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{interval, Duration},
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use uniswap_sdk_core::{prelude::*, token};
use uniswap_v3_sdk::prelude::sdk_core::prelude::{CurrencyAmount, U256, WETH9};
use uniswap_v3_sdk::prelude::*;

const MAINNET_RPC_WS: &str = "https://ethereum-rpc.publicnode.com";
const POLL_INTERVAL: Duration = Duration::from_secs(5); // Fetch price every 5s

pub(super) struct Builder {
    pub(super) shutdown_token: CancellationToken,
}

impl Builder {
    pub(super) fn build(self) -> Handle {
        let (tx, rx) = mpsc::channel(100); // Buffer size for the stream
        let worker = Worker::new(self.shutdown_token.clone(), tx);

        let worker_task = tokio::spawn(async move {
            worker.run().await;
        });

        Handle {
            worker_task,
            shutdown_token: self.shutdown_token,
            stream: ReceiverStream::new(rx), // Expose stream for price updates
        }
    }
}

pub(super) struct Handle {
    worker_task: JoinHandle<()>,
    shutdown_token: CancellationToken,
    pub stream: ReceiverStream<U256>,
}

impl Handle {
    pub async fn shutdown(self) {
        self.shutdown_token.cancel();
        if let Err(e) = self.worker_task.await {
            error!(%e, "Uniswap worker task failed");
        }
    }
}

pub(super) struct Worker {
    shutdown_token: CancellationToken,
    sender: mpsc::Sender<U256>,
}

impl Worker {
    pub fn new(shutdown_token: CancellationToken, sender: mpsc::Sender<U256>) -> Self {
        Self {
            shutdown_token,
            sender,
        }
    }

    async fn run(self) {
        let provider = ProviderBuilder::new().on_http(Url::from_str(MAINNET_RPC_WS).unwrap());
        let client = Arc::new(provider);

        let wbtc = token!(1, "2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599", 8, "WBTC");
        let weth = WETH9::on_chain(1).unwrap();

        let mut interval = interval(POLL_INTERVAL);
        let mut pool_task: Fuse<
            JoinHandle<Result<Pool<EphemeralTickMapDataProvider>, uniswap_v3_sdk::error::Error>>,
        > = Fuse::terminated();

        loop {
            tokio::select! {
                biased;
                _ = self.shutdown_token.cancelled() => {
                    debug!("Shutting down Uniswap worker...");
                    break;
                }
                res = &mut pool_task, if !pool_task.is_terminated() => {
                    match res {
                        Ok(Ok(pool)) => {
                            if let Err(e) = self.fetch_price(&wbtc, &weth, pool).await {
                                error!(%e, "Failed to fetch price");
                            }
                        }
                        Ok(Err(e)) => error!(%e, "Failed to get Uniswap V3 pool"),
                        Err(e) => error!(%e, "Task join error"),
                    }
                }
                _ = interval.tick(), if pool_task.is_terminated() => {
                    pool_task = tokio::spawn({
                        let client = client.clone();
                        let wbtc = wbtc.clone();
                        let weth = weth.clone();
                        async move {
                            Pool::<EphemeralTickMapDataProvider>::from_pool_key_with_tick_data_provider(
                                1,
                                FACTORY_ADDRESS,
                                wbtc.address(),
                                weth.address(),
                                FeeAmount::LOW,
                                client.clone(),
                                None
                            ).await
                        }
                    }).fuse();
                }
            }
        }
    }

    async fn fetch_price(
        &self,
        wbtc: &Token,
        weth: &Token,
        pool: Pool<EphemeralTickMapDataProvider>,
    ) -> eyre::Result<()> {
        let amount_in = CurrencyAmount::from_raw_amount(wbtc.clone(), 100_000_000)?;

        if let Ok(local_amount_out) = pool.get_output_amount(&amount_in, None) {
            let local_amount_out = local_amount_out.quotient();
            info!("New Uniswap V3 price: {:?}", local_amount_out);

            self.sender
                .send(U256::from_big_int(local_amount_out))
                .await?;
        }

        Ok(())
    }
}
