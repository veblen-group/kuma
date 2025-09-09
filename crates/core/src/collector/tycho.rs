//! Module for interacting with Tycho Simulation's ProtocolStream
use std::pin::Pin;

use color_eyre::eyre;
use color_eyre::eyre::WrapErr as _;
use tokio::{select, sync::broadcast};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, trace};
use tycho_simulation::evm::stream::ProtocolStreamBuilder;

use crate::{chain::Chain, state::tycho::BlockSim};

pub struct Handle {
    chain: Chain,
    shutdown_token: CancellationToken,
    worker_handle: Option<tokio::task::JoinHandle<eyre::Result<()>>>,
    block_sim_tx: broadcast::Sender<BlockSim>,
}

impl Handle {
    #[allow(unused)]
    pub async fn shutdown(&mut self) -> eyre::Result<()> {
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
    pub fn get_block_sim_rx(&self) -> broadcast::Receiver<BlockSim> {
        self.block_sim_tx.subscribe()
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

pub(super) struct Worker {
    pub(super) chain: Chain,
    pub(super) protocol_stream_builder: Pin<Box<dyn Future<Output = ProtocolStreamBuilder> + Send>>,
    pub(super) block_sim_tx: broadcast::Sender<BlockSim>,
    pub(super) shutdown_token: CancellationToken,
}

impl Worker {
    #[instrument(name = "tycho_stream_collector", skip(self), fields(chain.name = %self.chain.name))]
    pub async fn run(self) -> eyre::Result<()> {
        let Self {
            protocol_stream_builder,
            chain,
            shutdown_token,
            block_sim_tx,
        } = self;

        let mut protocol_stream = protocol_stream_builder
            .await
            .build()
            .await
            .wrap_err("Failed building protocol stream")?;

        info!(
            chain.name = ?chain.name,
            chain.id = ?chain.metadata.id(),
            "Initialized header and protocol streams"
        );

        let mut curr_block_sim: Option<BlockSim> = None;

        // TODO: combine headers, balances with protocol stream

        loop {
            select! {
                () = shutdown_token.cancelled() => {
                    info!("tycho collector received shutdown signal");
                    break Ok(())
                }

                Some(message_result) = protocol_stream.next() => {
                    let block_update = match message_result {
                        Ok(msg) => msg,
                        Err(e) => {
                            error!("Failed to receive message: {}", e);
                            continue;
                        }
                    };

                    debug!(
                        block.height = ?block_update.block_number_or_timestamp,
                        "🎁 Received block update"
                    );
                    let block = {
                        if let Some(old_block) = curr_block_sim.take() {
                            let new_block = old_block.apply_update(block_update);
                            trace!(
                                block.number = new_block.height,
                                "Applied block update from Tycho Simulation stream."
                            );

                            new_block
                        } else {
                            trace!(
                                block.number = block_update.block_number_or_timestamp,
                                "Received initial block from Tycho Simulation stream."
                            );
                            BlockSim::new(block_update)
                        }
                    };
                    if let Err(e) = block_sim_tx.send(block.clone()) {
                        error!("Failed to send block simulation update: {}", e);
                    } else {
                        curr_block_sim = Some(block);
                    }
                }
            }
        }
    }
}
