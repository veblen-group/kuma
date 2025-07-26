//! Module for interacting with Tycho Simulation's ProtocolStream
//! TODO: move this to a simulation submodule and add an execution submodule for the encoder
//! and submission stuff?
use std::{pin::Pin, sync::Arc};

use color_eyre::eyre;
use color_eyre::eyre::WrapErr as _;
use tokio::sync::watch;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument};
use tycho_simulation::evm::stream::ProtocolStreamBuilder;

use crate::{
    chain::Chain,
    state::{
        block::Block,
        pair::{Pair, PairStateStream},
    },
};

pub use builder::Builder;
mod builder;

pub struct Handle {
    #[allow(unused)]
    chain: Chain,
    #[allow(unused)]
    shutdown_token: CancellationToken,
    worker_handle: Option<tokio::task::JoinHandle<eyre::Result<()>>>,
    // TODO: get rid of option
    block_rx: watch::Receiver<Arc<Option<Block>>>,
}

impl Handle {
    #[allow(unused)]
    pub(crate) async fn shutdown(&mut self) -> eyre::Result<()> {
        self.shutdown_token.cancel();
        if let Err(e) = self
            .worker_handle
            .take()
            .expect("shutdown must not be called twice")
            .await
        {
            error!(chain=%self.chain, "Tycho simulation stream worker failed: {}", e);
            return Err(e.into());
        }
        Ok(())
    }

    #[allow(unused)]
    pub fn get_block_rx(&self) -> watch::Receiver<Arc<Option<Block>>> {
        self.block_rx.clone()
    }

    pub fn get_pair_state_stream(&self, pair: &Pair) -> PairStateStream {
        let block_rx = self.block_rx.clone();
        PairStateStream::from_block_rx(pair.clone(), block_rx)
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
            .expect("collector handle must not be polled after shutdown");

        task.poll_unpin(cx).map(|result| match result {
            Ok(worker_res) => match worker_res {
                Ok(()) => Ok(()),
                Err(e) => Err(e).wrap_err("collector task returned with err"),
            },
            Err(e) => Err(e).wrap_err("collector task panicked"),
        })
    }
}

struct Worker {
    chain: Chain,
    protocol_stream_builder: Pin<Box<dyn Future<Output = ProtocolStreamBuilder> + Send>>,
    block_tx: watch::Sender<Arc<Option<Block>>>,
}

impl Worker {
    #[instrument(name = "tycho_stream_collector", skip(self), fields(chain.name = %self.chain.name))]
    pub async fn run(self) -> eyre::Result<()> {
        let Self {
            protocol_stream_builder,
            chain,
            block_tx,
            ..
        } = self;

        let mut protocol_stream = protocol_stream_builder
            .await
            .build()
            .await
            .wrap_err("Failed building protocol stream")?;

        info!(
            chain.name = ?chain.name,
            chain.id = ?chain.metadata.id(),
            "Initialized protocol stream"
        );

        while let Some(message_result) = protocol_stream.next().await {
            let block_update = match message_result {
                Ok(msg) => msg,
                Err(e) => {
                    error!("Failed to receive message: {}", e);
                    continue;
                }
            };

            info!(
                block.height = ?block_update.block_number_or_timestamp,
                "🎁 Received block update"
            );
            let block = {
                if let Some(old_block) = block_tx.borrow().as_ref().clone() {
                    let new_block = old_block.apply_update(block_update);
                    info!(
                        block.number = new_block.height,
                        "Applied block update from Tycho Simulation stream."
                    );

                    Some(new_block)
                } else {
                    info!(
                        block.number = block_update.block_number_or_timestamp,
                        "Received initial block from Tycho Simulation stream."
                    );
                    Some(Block::new(block_update))
                }
            };
            let send_res = block_tx.send(Arc::new(block));
            if let Err(e) = send_res {
                // TODO: handle send_res more
                error!(err = %e, "Failed to receive block update from Tycho Simulation stream.");
            }
        }

        Ok(())
    }
}
