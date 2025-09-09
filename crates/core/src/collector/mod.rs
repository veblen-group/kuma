//! Module for interacting with Tycho Simulation's ProtocolStream
//! TODO: move this to a simulation submodule and add an execution submodule for the encoder
//! and submission stuff?
use std::{collections::HashMap, pin::Pin, str::FromStr, sync::Arc};

use alloy::{
    eips::BlockNumberOrTag,
    primitives::{Address, U256},
    providers::{Provider, ProviderBuilder, WsConnect},
    rpc::types::{Filter, Header},
    sol,
    sol_types::SolEvent as _,
};
use color_eyre::eyre;
use color_eyre::eyre::WrapErr as _;
use futures::{
    FutureExt as _,
    future::{Fuse, FusedFuture as _},
};
use num_bigint::BigUint;
use tokio::{select, sync::watch};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, trace};
use tycho_simulation::evm::stream::ProtocolStreamBuilder;

use crate::{
    chain::Chain,
    config::AddressForToken,
    state::{
        balances::TokenBalances,
        pair::{Pair, PairStateStream},
        tycho::BlockSim,
    },
};

pub use builder::Builder;
mod builder;
mod eth;
mod tycho;

pub struct Handle {
    #[allow(unused)]
    chain: Chain,
    shutdown_token: CancellationToken,
    worker_handle: Option<tokio::task::JoinHandle<eyre::Result<()>>>,
    // TODO: get rid of option
    block_rx: watch::Receiver<Arc<Option<BlockSim>>>,
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
    pub fn get_block_rx(&self) -> watch::Receiver<Arc<Option<BlockSim>>> {
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
    sim_tx: watch::Receiver<BlockSim>,
    eth_tx: watch::Receiver<(Header, TokenBalances)>,
    shutdown_token: CancellationToken,
    account_addr: Address,
    token_addrs: AddressForToken,
    ws_url: String,
}

impl Worker {
    #[instrument(name = "tycho_stream_collector", skip(self), fields(chain.name = %self.chain.name))]
    pub async fn run(self) -> eyre::Result<()> {
        let Self {
            chain,
            sim_tx,
            eth_tx,
            shutdown_token,
            account_addr,
            token_addrs,
            ws_url,
        } = self;

        let mut curr_eth_block = None;
        let mut curr_block_sim = None;

        // TODO: combine headers, balances with protocol stream

        loop {
            select! {
                () = shutdown_token.cancelled() => {
                    info!("tycho collector received shutdown signal");
                    break Ok(())
                }

                eth_block = eth_tx.changed() => {
                    if curr_block_sim.is_some() {
                        // TODO: send block on watch channel
                        block_tx.send(Arc::new(Some(Block::from_components())))
                    } else {
                        // TODO: update curr_block_sim
                    }
                }

                // TODO: fix this branch
                block_sim = sim_tx.changed() => {
                    if curr_eth_block.is_some() {
                        // TODO: send block on watch channel
                    } else {
                        // TODO: update curr_block_sim
                    }
                }
            }
        }
    }
}

fn send_block(
    tx: watch::Sender<Arc<Option<BlockSim>>>,
    curr_header: &Header,
    curr_block_sim: &BlockSim,
    curr_token_balances: &HashMap<Address, BigUint>,
) -> eyre::Result<()> {
    // TODO: send block on watch channel
    let block = BlockSim::from_components(curr_header, curr_block_sim, curr_token_balances.clone());
    let send_res = tx.send(Arc::new(Some(block)));
    if let Err(e) = send_res {
        // TODO: handle send_res more
        error!(err = %e, "Failed to receive block update from Tycho Simulation stream.");
    }
    Ok(())
}
