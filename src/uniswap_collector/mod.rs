use alloy::{
    providers::{Provider, ProviderBuilder},
    transports::http::reqwest::Url,
};

use futures::{
    future::{Fuse, FusedFuture as _, FutureExt as _},
    Future,
};
use std::{
    pin::Pin,
    str::FromStr,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{interval, Duration},
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use uniswap_sdk_core::{prelude::*, token};
use uniswap_v3_sdk::prelude::sdk_core::prelude::{U256, WETH9};
use uniswap_v3_sdk::prelude::*;

const MAINNET_RPC_WS: &str = "https://ethereum-rpc.publicnode.com";
const POLL_INTERVAL: Duration = Duration::from_secs(5); // Fetch price every 5s

struct PoolFut {
    inner: Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Pool<EphemeralTickMapDataProvider>,
                        uniswap_v3_sdk::error::Error,
                    >,
                > + Send,
        >,
    >,
}

impl PoolFut {
    fn new<T: Provider + 'static>(client: T, wbtc: Address, weth: Address) -> Self {
        let future = Pool::<EphemeralTickMapDataProvider>::from_pool_key_with_tick_data_provider(
            1,
            FACTORY_ADDRESS,
            wbtc,
            weth,
            FeeAmount::LOW,
            client,
            None,
        );
        Self {
            inner: Box::pin(future),
        }
    }
}

impl Future for PoolFut {
    type Output = Result<Pool<EphemeralTickMapDataProvider>, uniswap_v3_sdk::error::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.poll_unpin(cx)
    }
}

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

        let wbtc = token!(1, "2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599", 8, "WBTC").address();
        let weth = WETH9::on_chain(1).unwrap().address();
        let mut timer = interval(POLL_INTERVAL);
        let mut pool_task: Fuse<
            JoinHandle<Result<Pool<EphemeralTickMapDataProvider>, uniswap_v3_sdk::error::Error>>,
        > = tokio::task::spawn(PoolFut::new(client.clone(), wbtc, weth)).fuse();

        loop {
            tokio::select! {
                _ = self.shutdown_token.cancelled() => {
                    info!("Uniswap worker shutting down");
                    break;
                }
                _ = timer.tick(), if pool_task.is_terminated() => {
                    info!("timer ticked");
                    // restart the pool task
                    pool_task = tokio::task::spawn(PoolFut::new(
                        client.clone(),
                        wbtc,
                        weth,
                    )).fuse();
                }
                // Only poll if the future has not terminated
                join_result = &mut pool_task, if !pool_task.is_terminated() => {
                    match join_result {
                        Ok(pool) => {
                            match pool {
                                Ok(pool) => {
                                    let price = pool.token0_price().to_significant(5, None);
                                    info!("Price: {:?}", price);
                                },
                                Err(e) => {
                                    info!("Pool failed: {:?}", e);
                                }
                            }
                        }
                        Err(e) => {
                            info!("Task failed: {:?}", e);
                        }
                    }
                }
            }
        }
    }
}
