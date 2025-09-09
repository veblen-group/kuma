//! This module provides functionality to collect Ethereum block headers and token balances from an Ethereum node's JSON-RPC API.
//! Headers are collected from `eth_getBlock` and token balances are parsed from logs using the `TokenBalances` struct.
//!
//! The `Handle` struct represents a handle to the collector, allowing for shutdown, awaiting the worker's result
//! and getting a receiver for the latest block.
//! The `Worker` struct represents the worker that collects data from the Ethereum node's JSON-RPC API.
//!
//! The `EthBlock` type represents a block header and token balances.
//!
//! The `Handle` struct provides methods for shutting down the collector and awaiting the worker's result.
//! The `Future` trait implementation for the `Handle` struct allows for awaiting the worker's result.

use std::{pin::Pin, str::FromStr, sync::Arc};

use alloy::{
    primitives::Address,
    providers::{Provider, ProviderBuilder, WsConnect},
    rpc::types::Header,
};
use color_eyre::eyre::WrapErr as _;
use color_eyre::eyre::{self, eyre};
use tokio::{
    pin, select,
    sync::{Mutex, watch},
};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument};

use crate::{chain::Chain, config::AddressForToken, state::balances::TokenBalances};

pub type EthBlock = (Header, TokenBalances);

pub struct Handle {
    chain: Chain,
    shutdown_token: CancellationToken,
    worker_handle: Option<tokio::task::JoinHandle<eyre::Result<()>>>,
    latest_block_rx: watch::Receiver<EthBlock>,
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
    chain: Chain,
    latest_block_tx: watch::Sender<EthBlock>,
    shutdown_token: CancellationToken,
    account_addr: Address,
    token_addrs: AddressForToken,
    ws_url: String,
}

impl Worker {
    #[instrument(name = "tycho_stream_collector", skip(self), fields(chain.name = %self.chain.name))]
    pub async fn run(self) -> eyre::Result<()> {
        let Self {
            latest_block_tx,
            shutdown_token,
            account_addr,
            token_addrs,
            ws_url,
            ..
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

        let curr_token_balances = Arc::new(Mutex::new(
            TokenBalances::get_curr_balances(account_addr, addrs, provider.clone()).await?,
        ));

        // TODO: print this nicely
        debug!(?curr_token_balances, "Initialized token balances");

        // set up header stream
        let headers = provider.clone().subscribe_blocks().await?.into_stream();
        let headers_and_blocks = headers
            .then(|header| process_header(header, provider.clone(), curr_token_balances.clone()));
        pin!(headers_and_blocks);

        loop {
            select! {
                () = shutdown_token.cancelled() => {
                    info!("Eth Collector received shutdown signal");
                    break Ok(())
                }

                Some(res) = headers_and_blocks.next() => {
                    match res {
                        Ok((header, token_balances)) => {
                            debug!(block_height = ?header.number, "token balances updated");
                            if latest_block_tx.send((header, token_balances.clone())).is_err() {
                                error!("Watch channel closed, shutting down collector.");
                                break Ok(());
                            }
                        }
                        Err(e) => {
                            break Err(eyre!("Balance update failed: {}", e));
                       }
                    }
                }
            }
        }
    }
}

async fn process_header(
    header: Header,
    provider: impl Provider + Clone,
    token_balances: Arc<Mutex<TokenBalances>>,
) -> eyre::Result<EthBlock> {
    let mut token_balances_guard = token_balances.lock().await;
    token_balances_guard
        .update_from_logs(provider.clone())
        .await
        .wrap_err_with(|| {
            format!(
                "Failed to update token balances for block {}",
                header.number
            )
        })?;

    debug!(block_height = %header.number, "Received header");
    Ok((header, token_balances_guard.clone()))
}
