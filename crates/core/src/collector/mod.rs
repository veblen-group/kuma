//! This collector multiplexes data from the Ethereum JSON RPC collector and the Tycho simulation stream.
//! It provides a simplified handle for getting blocks, or pair-specific state updates.
use std::{pin::Pin, sync::Arc};

use alloy::rpc::types::Header;
use color_eyre::eyre;
use color_eyre::eyre::WrapErr as _;
use tokio::{
    select,
    sync::{
        broadcast::{self, error::RecvError},
        watch,
    },
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument};

use crate::{
    chain::Chain,
    collector::eth::EthBlock,
    state::{
        balances::TokenBalances,
        block::Block,
        pair::{Pair, PairStateStream},
        tycho::BlockSim,
    },
};

pub use builder::Builder;
mod builder;
mod eth;
mod tycho;

pub struct Handle {
    chain: Chain,
    shutdown_token: CancellationToken,
    worker_handle: Option<tokio::task::JoinHandle<eyre::Result<()>>>,
    block_rx: watch::Receiver<Arc<Option<Block>>>,
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
            error!(chain=%self.chain, "Tycho simulation stream worker failed: {}", e);
            return Err(e.into());
        }
        Ok(())
    }

    pub fn get_block_rx(&self) -> watch::Receiver<Arc<Option<Block>>> {
        self.block_rx.clone()
    }

    pub fn get_pair_state_stream(&self, pair: &Pair) -> PairStateStream {
        let block_rx = self.block_rx.clone();
        PairStateStream::from_block_rx(pair.clone(), block_rx)
    }
}

/// Awaiting the handle deals with the Worker's result
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
    block_sim_rx: broadcast::Receiver<BlockSim>,
    eth_rx: broadcast::Receiver<(Header, TokenBalances)>,
    block_tx: watch::Sender<Arc<Option<Block>>>,
    shutdown_token: CancellationToken,
}

impl Worker {
    #[instrument(name = "block_collector", skip(self), fields(chain.name = %self.chain.name))]
    pub async fn run(self) -> eyre::Result<()> {
        let Self {
            mut block_sim_rx,
            mut eth_rx,
            block_tx,
            shutdown_token,
            ..
        } = self;

        let mut curr_eth_block: Option<EthBlock> = None;
        let mut curr_block_sim: Option<BlockSim> = None;

        loop {
            select! {
                () = shutdown_token.cancelled() => {
                    info!("block collector received shutdown signal");
                    break Ok(())
                }

                res = eth_rx.recv() => {
                    match res {
                        Ok((header, token_balances)) => {
                            if let Some(block_sim) = curr_block_sim.take() {
                                if header.number == block_sim.height {
                                    let block = Block::from_components(header, token_balances, block_sim);

                                    if let Err(e) = block_tx.send(Arc::new(Some(block))) {
                                        // TODO: handle send_res more
                                        error!(err = %e, "failed to send block after receiving eth block");
                                    }
                                } else {
                                    error!(
                                        eth_block.height = header.number,
                                        sim_block.height = block_sim.height,
                                        "Block heights out of order"
                                    );
                                }
                            } else {
                                curr_eth_block = Some((header, token_balances));
                            }
                        },
                        Err(RecvError::Lagged(missed_count)) => {
                            error!(missed_count = missed_count, "Tycho Simulation stream lagged");
                        },
                        Err(RecvError::Closed) => {
                            error!("EthBlock channel closed, shutting down");
                            break Ok(());
                        },
                    }
                }

                res = block_sim_rx.recv() => {
                    match res {
                        Ok(block_sim) => {
                            if let Some((header, token_balances)) = curr_eth_block.take() {
                                if header.number == block_sim.height {
                                    let block = Block::from_components(header, token_balances, block_sim);

                                    let send_res = block_tx.send(Arc::new(Some(block)));
                                    if let Err(e) = send_res {
                                        // TODO: handle send_res more
                                        error!(err = %e, "failed to send block after receiving tycho block");
                                    }
                                } else {
                                    error!(eth_block.height = header.number, sim_block.height = block_sim.height, "Block heights out of order");
                                }
                                curr_eth_block = None;
                                curr_block_sim = None;
                            } else {
                                curr_block_sim = Some(block_sim);
                            }
                        },
                        Err(RecvError::Lagged(missed_count)) => {
                            error!(missed_count = missed_count, "Tycho Simulation stream lagged");
                        },
                        Err(RecvError::Closed) => {
                            error!("Tycho Simulation stream closed, shutting down");
                            break Ok(())
                        },
                    }
                }
            }
        }
    }
}
