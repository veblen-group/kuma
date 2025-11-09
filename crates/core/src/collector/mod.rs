//! This collector multiplexes data from the Ethereum JSON RPC collector and the Tycho simulation stream.
//! It provides a simplified handle for getting blocks, or pair-specific state updates.
use std::{pin::Pin, sync::Arc};

use alloy::rpc::types::Header;
use color_eyre::eyre::{self};
use color_eyre::eyre::{WrapErr as _, bail};
use tokio::{select, sync::watch};
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::WatchStream;
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

                Some(Some((header, token_balances))) = self.eth_rx.next() => {
                    let Some(block_sim) = self.curr_block_sim.take() else {
                        self.curr_eth_block = Some((header, token_balances));
                        continue;
                    };

                    self.send_block(header, token_balances, block_sim)?;

                    self.curr_eth_block = None;
                    self.curr_block_sim = None;
                }

                Some(Some(block_sim)) = self.block_sim_rx.next() => {
                    let Some((header, token_balances)) = self.curr_eth_block.take() else {
                        self.curr_block_sim = Some(block_sim);
                        continue;
                    };

                    self.send_block(header, token_balances, block_sim)?;

                    self.curr_eth_block = None;
                    self.curr_block_sim = None;
                }
            }
        }
    }

    fn send_block(
        &mut self,
        header: Header,
        token_balances: TokenBalances,
        block_sim: BlockSim,
    ) -> eyre::Result<()> {
        if header.number != block_sim.height {
            error!(
                eth_height = %header.number,
                tycho_height = %block_sim.height,
                "Block heights are out of order"
            );
            bail!("Block heights out of order");
        }
        let block = Block::from_components(header, token_balances, block_sim);
        let height = block.header.number;

        if let Err(error) = self.block_tx.send(Arc::new(Some(block))) {
            bail!("failed to send block after receiving tycho block: {error}")
        } else {
            info!(block.height = %height, "🎁 Collected new block");
            Ok(())
        }
    }
}
