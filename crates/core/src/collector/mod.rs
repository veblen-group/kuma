//! This collector multiplexes data from the Ethereum JSON RPC collector and the Tycho simulation stream.
//! It provides a simplified handle for getting blocks, or pair-specific state updates.
use std::{pin::Pin, sync::Arc};

use color_eyre::eyre::{self};
use color_eyre::eyre::{WrapErr as _, bail};
use tokio::{select, sync::watch};
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::WatchStream;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument, warn};

use crate::{
    chain::Chain,
    collector::eth::EthBlock,
    state::{
        block::Block,
        pair::{Pair, PairStateStream},
        tycho::BlockSim,
    },
};

pub use builder::Builder;

mod builder;
pub mod eth;
pub mod tycho;

pub struct Handle {
    chain: Chain,
    shutdown_token: CancellationToken,
    task_handle: Option<tokio::task::JoinHandle<eyre::Result<()>>>,
    block_rx: watch::Receiver<Arc<Option<Block>>>,
}

impl Handle {
    pub async fn shutdown(&mut self) -> eyre::Result<()> {
        self.shutdown_token.cancel();
        if let Err(e) = self
            .task_handle
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
            .task_handle
            .as_mut()
            .expect("collector handle must not be polled after shutdown");

        task.poll_unpin(cx).map(|result| match result {
            Ok(worker_res) => match worker_res {
                Ok(()) => Ok(()),
                Err(e) => Err(e),
            },
            Err(e) => Err(e).wrap_err("block collector task panicked"),
        })
    }
}

struct Worker {
    chain: Chain,
    block_sim_rx: WatchStream<Option<BlockSim>>,
    eth_rx: WatchStream<Option<EthBlock>>,
    block_tx: watch::Sender<Arc<Option<Block>>>,
    shutdown_token: CancellationToken,
    curr_eth_block: Option<EthBlock>,
    curr_block_sim: Option<BlockSim>,
    collector_lag_tolerance: i64,
}

impl Worker {
    #[instrument(name = "block_collector", skip(self), fields(chain.name = %self.chain.name))]
    pub async fn run(mut self) -> eyre::Result<()> {
        loop {
            select! {
                () = self.shutdown_token.cancelled() => {
                    info!("block collector received shutdown signal");
                    break Ok(())
                }

                Some(Some(eth_block)) = self.eth_rx.next() => {
                    let Some(block_sim) = self.curr_block_sim.take() else {
                        self.curr_eth_block = Some(eth_block);
                        continue
                    };

                    if let Some(block) = self.try_block_from_components(eth_block, block_sim)? {
                        self.try_send_block(block)?;
                    }
                }

                Some(Some(block_sim)) = self.block_sim_rx.next() => {
                    let Some(eth_block) = self.curr_eth_block.take() else {
                        self.curr_block_sim = Some(block_sim);
                        continue;
                    };

                    if let Some(block) = self.try_block_from_components(eth_block, block_sim)? {
                        self.try_send_block(block)?;
                    }
                }
            }
        }
    }

    #[instrument(skip_all, fields(eth_block.height = %header.number, block_sim.height = %block_sim.height))]
    fn try_block_from_components(
        &mut self,
        (header, token_balances): EthBlock,
        block_sim: BlockSim,
    ) -> eyre::Result<Option<Block>> {
        let height_diff = header.number as i64 - block_sim.height as i64;
        if height_diff > 0 {
            warn!("Block heights are out of order, Tycho is lagging behind");

            if height_diff > self.collector_lag_tolerance {
                error!(
                    lag = height_diff,
                    tolerance = self.collector_lag_tolerance,
                    "Tycho surpassed collector lag tolerance, shutting down"
                );
                bail!(
                    "Tycho collector is lagging behind by {} blocks",
                    height_diff
                )
            };

            Ok(None)
        } else if height_diff < 0 {
            warn!("Block heights are out of order, Eth is lagging behind");
            self.curr_eth_block = None;
            self.curr_block_sim = Some(block_sim);

            if height_diff < (self.collector_lag_tolerance * -1) {
                error!(
                    lag = height_diff,
                    tolerance = self.collector_lag_tolerance,
                    "Eth surpassed collector lag tolerance, shutting down",
                );
                bail!(
                    "Eth collector is lagging behind by more than {} blocks",
                    self.collector_lag_tolerance
                )
            }

            Ok(None)
        } else {
            Ok(Some(Block::from_components(
                header,
                token_balances,
                block_sim,
            )))
        }
    }

    fn try_send_block(&mut self, block: Block) -> eyre::Result<()> {
        let height = block.header.number;

        if let Err(error) = self.block_tx.send(Arc::new(Some(block))) {
            bail!("failed to send block after receiving tycho block: {error}")
        } else {
            info!(block.height = %height, "🎁 Collected new block");

            self.curr_eth_block = None;
            self.curr_block_sim = None;

            Ok(())
        }
    }
}
