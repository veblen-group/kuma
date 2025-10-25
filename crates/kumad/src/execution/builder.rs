use color_eyre::eyre::{self};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use kuma_core::{database, signals};

use super::{Handle, Worker};

pub struct Builder {
    pub signal_rxs: Vec<broadcast::Receiver<signals::CrossChainSingleHop>>,
    pub db: database::Handle,
}

impl Builder {
    pub fn build(self) -> eyre::Result<Handle> {
        let Self { signal_rxs, db } = self;

        let shutdown_token = CancellationToken::new();

        let worker = Worker {
            signal_rxs,
            shutdown_token: shutdown_token.clone(),
            db,
        };

        let worker_handle = tokio::task::spawn(async move { worker.run().await });

        Ok(Handle {
            shutdown_token,
            worker_handle: Some(worker_handle),
        })
    }
}
