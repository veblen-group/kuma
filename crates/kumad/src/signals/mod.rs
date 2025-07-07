use std::time::Duration;

use color_eyre::eyre;
use tokio::{select, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{binance, uniswap};

pub(super) struct Builder {
    pub(super) poll_interval_ms: u64,
    pub(super) shutdown_token: CancellationToken,
    binance_handle: binance::Handle,
    uniswap_handle: uniswap::Handle,
}

impl Builder {
    pub(super) fn build(self) -> Handle {
        let Builder {
            poll_interval_ms,
            shutdown_token,
            binance_handle,
            uniswap_handle,
            ..
        } = self;

        let worker = Worker {
            poll_interval: Duration::from_millis(poll_interval_ms),
            shutdown_token: shutdown_token.clone(),
            binance_handle,
            uniswap_handle,
        };

        let worker_task = tokio::spawn(async move { worker.run().await });

        Handle {
            worker_task,
            shutdown_token,
        }
    }
}

pub(super) struct Handle {
    worker_task: JoinHandle<eyre::Result<()>>,
    shutdown_token: CancellationToken,
}

impl Handle {
    pub(super) async fn shutdown(self) -> eyre::Result<()> {
        todo!("implement me")
    }
}

pub(super) struct Worker {
    poll_interval: Duration,
    shutdown_token: CancellationToken,
    binance_handle: binance::Handle,
    uniswap_handle: uniswap::Handle,
}

impl Worker {
    async fn run(self) -> eyre::Result<()> {
        let mut timer = tokio::time::interval(self.poll_interval);

        loop {
            select! {
                _ = self.shutdown_token.cancelled() => {
                    info!("Signal worker received shutdown signal");
                    break;
                }

                _ = timer.tick() => {
                    info!("Calculating arbitrage signals");
                    let binance_price = self.binance_handle.get_curr_price();
                    let uniswap_price = self.uniswap_handle.get_curr_price();
                }
            }
        }
        // timer tick
        //  read prices
        //  calculate arbitrage
        //  emit BinanceFlameArbitrage signal

        todo!("implement me")
    }
}

pub(super) enum TIAUSDCFlameBinance {
    BuyTiaOnFlame,  // SellTiaOnBinance
    SellTiaOnFlame, // BuyTiaOnBinance
}

pub(super) struct BinanceFlameArbitrage {
    binance_trade: BinanceTIATrade,
    flame_trade: FlameTrade,
    direction: TIAUSDCFlameBinance,
}

pub(super) struct BinanceTIATrade {
    amount: f64,
    price: f64,
}

pub(super) struct FlameTrade {
    amount: f64,
    slippage: f64,
}
