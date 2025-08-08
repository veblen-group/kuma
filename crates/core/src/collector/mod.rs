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
        block::Block,
        pair::{Pair, PairStateStream},
    },
};

pub use builder::Builder;
mod builder;

// Taken from https://github.com/OpenZeppelin/openzeppelin-contracts/blob/3790c59623e99cb0272ddf84e6a17a5979d06b35/contracts/token/ERC20/IERC20.sol
sol!(
    #[sol(rpc)]
    contract IERC20 {
        function balanceOf(address account) external view returns (uint256);
        event Transfer(address indexed from, address indexed to, uint256 value);
    }
);

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
    shutdown_token: CancellationToken,
    account_addr: Address,
    token_addrs: AddressForToken,
    ws_url: String,
}

impl Worker {
    #[instrument(name = "tycho_stream_collector", skip(self), fields(chain.name = %self.chain.name))]
    pub async fn run(self) -> eyre::Result<()> {
        let Self {
            protocol_stream_builder,
            chain,
            block_tx,
            shutdown_token,
            account_addr,
            token_addrs,
            ws_url,
        } = self;

        let ws = WsConnect::new(ws_url);
        let provider = ProviderBuilder::new().connect_ws(ws).await?;

        let addrs = token_addrs
            .keys()
            .map(|addr_bytes| {
                let addr = Address::from_str(&addr_bytes.to_string())
                    .wrap_err("Failed to parse address")?;
                Ok(addr)
            })
            .collect::<eyre::Result<Vec<_>>>()?;

        // get token contract handle
        let tokens = addrs
            .iter()
            .cloned()
            .map(|addr| IERC20::new(addr, provider.clone()))
            .collect::<Vec<_>>();

        // get initial balances
        let mut curr_token_balances = HashMap::new();
        for token in &tokens {
            let start: U256 = token.balanceOf(account_addr).call().await?;
            let current_balance = BigUint::from_bytes_be(&start.to_be_bytes::<32usize>());
            curr_token_balances.insert(token.address().clone(), current_balance);
        }
        // pin!(curr_token_balances);

        // TODO: print this nicely
        debug!(?curr_token_balances, "Initialized token balances");

        let from_filter = Filter::new()
            .address(addrs.clone())
            .event(IERC20::Transfer::SIGNATURE)
            .topic1(account_addr)
            .from_block(BlockNumberOrTag::Latest);
        let to_filter = Filter::new()
            .address(addrs.clone())
            .event(IERC20::Transfer::SIGNATURE)
            .topic2(account_addr)
            .from_block(BlockNumberOrTag::Latest);

        // set up header stream
        let mut headers = provider.clone().subscribe_blocks().await?.into_stream();

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

        let mut transfer_fetch = Fuse::terminated();

        let mut curr_header = None;
        let mut curr_block_sim = None;

        // TODO: combine headers, balances with protocol stream

        loop {
            select! {
                () = shutdown_token.cancelled() => {
                    info!("tycho collector received shutdown signal");
                    break Ok(())
                }

                res = transfer_fetch, if !transfer_fetch.is_terminated() => {
                    match res {
                        Ok(_) => {
                            debug!("transfer fetch completed");
                            if let (Some(mut header), Some(mut block_sim)) = (&curr_header, &curr_block_sim) {
                                send_block(self.block_tx.clone(), header, block_sim, &curr_token_balances);
                            }
                        }
                        Err(e) => {
                            error!(error = %e, "transfer fetch failed");
                        }
                    }

                }

                Some(header) = headers.next() => {
                    curr_header = Some(header);
                    transfer_fetch = update_token_balances(&mut curr_token_balances, to_filter.clone(), from_filter.clone(), provider.clone()).fuse();
                    debug!("Received header");
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
                    curr_block_sim = {
                        if let Some(old_block) = block_tx.borrow().as_ref().clone() {
                            let new_block = old_block.apply_update(block_update);
                            trace!(
                                block.number = new_block.height,
                                "Applied block update from Tycho Simulation stream."
                            );

                            Some(new_block)
                        } else {
                            trace!(
                                block.number = block_update.block_number_or_timestamp,
                                "Received initial block from Tycho Simulation stream."
                            );
                            Some(Block::new(block_update))
                        }
                    };
                    // TODO: put the header, balances, and tycho block sim in the watch and set locals to none

                }
            }
        }
    }
}

async fn update_token_balances<P: Provider + Clone>(
    curr_token_balances: &mut HashMap<Address, BigUint>,
    to_filter: Filter,
    from_filter: Filter,
    provider: P,
) -> eyre::Result<()> {
    let to_logs = provider
        .get_logs(&to_filter)
        .await
        .wrap_err("failed to get transfer logs to account addr")?;

    for log in to_logs {
        let event = log
            .log_decode::<IERC20::Transfer>()
            .wrap_err("failed to parse transfer event")?;
        let IERC20::Transfer { from: _, to, value } = event.inner.data;
        // TODO: update curr_balances
        let value = BigUint::from_bytes_be(&value.to_be_bytes::<32>());
        let curr = curr_token_balances
            .entry(to)
            .and_modify(|curr| *curr += value);
    }

    let from_logs = provider
        .get_logs(&from_filter)
        .await
        .wrap_err("failed to get transfer logs from account addr")?;

    // TODO: process logs to update token balances
    Ok(())
}

fn send_block(
    tx: watch::Sender<Arc<Option<Block>>>,
    curr_header: Header,
    curr_block_sim: Block,
    curr_token_balances: &HashMap<Address, BigUint>,
) -> eyre::Result<()> {
    // TODO: send block on watch channel
    let block = Block::from_components(curr_header, curr_block_sim, curr_token_balances.clone());
    let send_res = tx.send(Arc::new(Some(block)));
    if let Err(e) = send_res {
        // TODO: handle send_res more
        error!(err = %e, "Failed to receive block update from Tycho Simulation stream.");
    }
    Ok(())
}
