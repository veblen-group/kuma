use std::sync::{Arc, atomic::AtomicBool};

use color_eyre::eyre::{self, Context as _};
use futures::StreamExt as _;
use serde::Serialize;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

pub struct Handle {
    pub(super) worker_task: JoinHandle<eyre::Result<()>>,
    pub(super) shutdown_token: CancellationToken,
}

impl Handle {
    pub(crate) async fn shutdown(self) -> eyre::Result<()> {
        self.shutdown_token.cancel();

        if let Err(e) = self.worker_task.await {
            error!(%e, "binance worker task failed");
        }

        Ok(())
    }

    pub(crate) fn get_curr_price(&self) -> f64 {
        0.0
    }
}

pub struct Worker {
    pub(super) shutdown_token: CancellationToken,
    pub(super) keep_running: Arc<AtomicBool>,
    pub(super) rpc_url: String,
    pub(super) markets: Vec<String>,
}

impl Worker {
    pub(super) async fn run(self) -> eyre::Result<()> {
        // TODO: use subscribe message instead of putting them all in the url?
        let uri = format!("{}/stream?streams={}", self.rpc_url, self.markets.join("/"));
        // binance responds with empty json so we can throw that away
        let (socket, _) = tokio_tungstenite::connect_async(&uri)
            .await
            .wrap_err("failed to connect to binance websocket")?;

        let (_write, mut read) = socket.split();

        // TODO: send subscribe messsage
        let _subscribe_msg = serde_json::to_string(&{
            #[derive(Serialize)]
            struct SubscribeMessage {
                method: String,
                params: Vec<String>,
                id: u32,
            }

            SubscribeMessage {
                method: "SUBSCRIBE".into(),
                params: self
                    .markets
                    .into_iter()
                    .map(|ticker_name| format!("{}@bookTicker", ticker_name))
                    .collect(),
                id: 1,
            }
        });

        loop {
            tokio::select! {
                    () = self.shutdown_token.cancelled() => {
                        debug!("cancelling binance worker");
                        self.keep_running.store(false, std::sync::atomic::Ordering::Relaxed);
                        break Ok(())
                    }
                    // TODO: pull from the tungtenite stream isntead
                    Some(msg) = read.next() => {
                        let msg = msg.wrap_err("binance websocket stream closed")?;
                        match msg {
                            Message::Text(utf8_bytes) => todo!(),
                            Message::Binary(bytes) => todo!(),
                            Message::Ping(bytes) => todo!(),
                            Message::Pong(bytes) => todo!(),
                            Message::Close(close_frame) => todo!(),
                            Message::Frame(frame) => todo!(),
                        }
                    }
            }
        }
    }
}
