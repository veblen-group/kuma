//! Builder for the block-collector pipeline.
//!
//! [`Builder`] wires the two low-level collectors — an Ethereum node WebSocket
//! collector ([`eth::Builder`]) and a Tycho protocol-state collector
//! ([`tycho::Builder`]) — together behind a block multiplexer ([`collector::Worker`]).
//!
//! The multiplexer aligns blocks from both sources (tolerance:
//! `COLLECTOR_LAG_TOLERANCE = 1` block) and publishes merged [`Block`] values on
//! a `watch` channel. Downstream strategies subscribe to this channel via the
//! returned [`Handle`].
//!
//! Returns three handles so callers can monitor each worker independently:
//! `(collector::Handle, eth::Handle, tycho::Handle)`.

use std::{collections::HashMap, sync::Arc};

use color_eyre::eyre::{self, Context as _};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tycho_simulation::tycho_common::{Bytes, models::token::Token};

use crate::{
    chain::Chain,
    collector::{
        self,
        eth::{self},
        tycho,
    },
    state::block::Block,
};

const COLLECTOR_LAG_TOLERANCE: i64 = 1;

pub struct Builder {
    pub chain: Chain,
    pub tycho_url: String,
    pub tycho_api_key: String,
    pub token_addrs: HashMap<Bytes, Token>,
    pub add_tvl_threshold: f64,
    pub remove_tvl_threshold: f64,
    pub shutdown_token: CancellationToken,
}

impl Builder {
    pub fn build(self) -> eyre::Result<(super::Handle, eth::Handle, tycho::Handle)> {
        let Self {
            tycho_url,
            add_tvl_threshold,
            remove_tvl_threshold,
            chain,
            tycho_api_key,
            token_addrs,
            shutdown_token,
            ..
        } = self;

        // TODO: move this stuff into the tycho builder

        let (block_tx, block_rx) = watch::channel::<Arc<Option<Block>>>(Arc::new(None));

        let eth_worker_handle = eth::Builder {
            chain: chain.clone(),
            shutdown_token: shutdown_token.clone(),
            account_addr: chain.signer().address(),
            token_addrs: token_addrs.clone(),
            ws_url: chain.rpc_ws_url.clone(),
        }
        .build();

        let tycho_worker_handle = tycho::Builder {
            chain: chain.clone(),
            tycho_url,
            add_tvl_threshold,
            remove_tvl_threshold,
            tycho_api_key,
            token_addrs,
            shutdown_token: shutdown_token.clone(),
        }
        .build()
        .wrap_err("failed to build tycho collector worker")?;

        let worker = collector::Worker {
            chain: chain.clone(),
            block_tx,
            shutdown_token: shutdown_token.clone(),
            block_sim_rx: tycho_worker_handle.get_block_sim_rx(),
            eth_rx: eth_worker_handle.get_block_changes_stream(),
            curr_block_sim: None,
            curr_eth_block: None,
            collector_lag_tolerance: COLLECTOR_LAG_TOLERANCE,
        };
        let worker_task = tokio::task::spawn(async { worker.run().await });

        // TODO: return tycho and eth handles
        let block_worker_handle = super::Handle {
            chain,
            shutdown_token,
            task_handle: Some(worker_task),
            block_rx,
        };

        Ok((block_worker_handle, eth_worker_handle, tycho_worker_handle))
    }
}
