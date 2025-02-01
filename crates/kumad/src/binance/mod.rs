use std::sync::{Arc, atomic::AtomicBool};

use binance::model::BookTickerEvent;
use color_eyre::eyre::{self, Context as _};

mod worker;

use tokio_util::sync::CancellationToken;
pub use worker::Handle;

use crate::binance::worker::Worker;

#[derive(Debug)]
pub(super) struct BookTicker {
    update_id: u64,
    symbol: String,
    best_bid: u64,
    best_bid_qty: u64,
    best_ask: u64,
    best_ask_qty: u64,
}

impl BookTicker {
    // from websocket?
    pub fn from_binance_websocket_event(raw: BookTickerEvent) -> eyre::Result<Self> {
        Ok(Self {
            update_id: raw.update_id,
            symbol: raw.symbol,
            best_bid: raw.best_bid.parse().wrap_err("failed to parse best bid")?,
            best_bid_qty: raw
                .best_bid_qty
                .parse()
                .wrap_err("failed to parse best bid qty")?,
            best_ask: raw.best_ask.parse().wrap_err("failed to parse best ask")?,
            best_ask_qty: raw
                .best_ask_qty
                .parse()
                .wrap_err("failed to parse best ask qty")?,
        })
    }
}

pub(crate) struct Builder {
    pub(crate) shutdown_token: CancellationToken,
    pub(crate) markets: Vec<String>,
    pub(crate) rpc_url: String,
}

impl Builder {
    pub(crate) fn build(self) -> Handle {
        let Self {
            shutdown_token,
            rpc_url,
            markets,
            ..
        } = self;
        let keep_running = Arc::new(AtomicBool::new(true));

        let worker = Worker {
            shutdown_token: shutdown_token.clone(),
            keep_running,
            rpc_url,
            markets,
        };
        let worker_task = tokio::spawn(async { worker.run().await });

        Handle {
            worker_task,
            shutdown_token,
        }
    }
}
