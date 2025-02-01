// TODO:
// 1. client wrapper
// 2. data types
// 3. streams for data types

use std::sync::{atomic::AtomicBool, Arc};

use binance::websockets::{WebSockets, WebsocketEvent};
use color_eyre::eyre::{self, Context};
use futures::{future::FusedFuture as _, FutureExt};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

mod book_ticker;

pub(super) struct Builder {
    pub(super) shutdown_token: CancellationToken,
    pub(super) markets: Vec<String>,
}

impl Builder {
    // TODO: get rid of this and move it to Binance:: spawn()?
    pub(super) fn build(self) -> Handle {
        let Self {
            shutdown_token,
            markets,
            ..
        } = self;
        let keep_running = Arc::new(AtomicBool::new(true));

        let worker = Worker {
            shutdown_token: shutdown_token.clone(),
            keep_running,
            markets,
        };
        let worker_task = tokio::spawn(async { worker.run().await });

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
        self.shutdown_token.cancel();

        if let Err(e) = self.worker_task.await {
            error!(%e, "binance worker task failed");
        }

        Ok(())
    }

    pub fn get_curr_price(&self) -> f64 {
        0.0
    }
}

pub(super) struct Worker {
    shutdown_token: CancellationToken,
    keep_running: Arc<AtomicBool>,
    markets: Vec<String>,
}

impl Worker {
    async fn run(self) -> eyre::Result<()> {
        let mut ws_task = {
            let keep_running = self.keep_running.clone();
            let endpoint = self.markets[0].clone();
            tokio::spawn(async move {
                let mut ws = WebSockets::new(|event: WebsocketEvent| {
                    match event {
                        // TODO: add handlers for specific events
                        WebsocketEvent::BookTicker(raw) => {
                            debug!(?raw, "raw bookticker event");
                            // TODO: clean up into domain object
                            let bt = book_ticker::BookTicker::from_binance_websocket_event(raw);
                            debug!(?bt, "bookticker");
                        }
                        _ => (),
                    };
                    Ok(())
                });

                // TODO: move this to binance ws client module/type
                if let Err(e) = ws.connect(&endpoint) {
                    error!(%e, ?endpoint, "failed to connect to binance ws");
                }
                debug!("connceted to binance ws");

                if let Err(e) = ws.event_loop(&keep_running) {
                    error!(%e, "websocket event loop failed");
                }
                debug!("exited websocket event loop");

                ws.disconnect().expect("what is this");
                debug!("disconnected from binance ws");
            })
            .fuse()
        };

        loop {
            tokio::select! {
                    () = self.shutdown_token.cancelled() => {
                        debug!("cancelling binance worker");
                        self.keep_running.store(false, std::sync::atomic::Ordering::Relaxed);
                        break Ok(())
                    }
                    res = &mut ws_task, if ws_task.is_terminated() =>{
                        break res.wrap_err("websocket loop crashed")
                    }
            }
        }
    }
}
