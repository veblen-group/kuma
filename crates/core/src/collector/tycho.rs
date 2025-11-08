//! Module for interacting with Tycho Simulation's ProtocolStream
use std::collections::HashMap;
use std::pin::Pin;

use color_eyre::eyre::{self, WrapErr as _, bail, eyre};
use tokio::sync::watch::error::SendError;
use tokio::{select, sync::watch};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::WatchStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, trace};
use tycho_common::Bytes;
use tycho_common::models::token::Token;
use tycho_simulation::evm::protocol::pancakeswap_v2::state::PancakeswapV2State;
use tycho_simulation::evm::protocol::uniswap_v2::state::UniswapV2State;
use tycho_simulation::evm::protocol::uniswap_v3::state::UniswapV3State;
use tycho_simulation::evm::stream::ProtocolStreamBuilder;
use tycho_simulation::tycho_client::feed::component_tracker::ComponentFilter;

use crate::{chain::Chain, state::tycho::BlockSim};

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
    pub fn build(self) -> eyre::Result<Handle> {
        let Self {
            chain,
            shutdown_token,
            tycho_url,
            tycho_api_key,
            token_addrs,
            add_tvl_threshold,
            remove_tvl_threshold,
        } = self;

        // make protocol stream
        let protocol_stream = ProtocolStreamBuilder::new(&tycho_url, chain.name);
        let tvl_filter = ComponentFilter::with_tvl_range(remove_tvl_threshold, add_tvl_threshold);
        let protocol_stream = Self::add_exchanges_for_chain(&chain, protocol_stream, tvl_filter)
            .wrap_err("failed to set exchanges for {chain.name}.")?;

        let protocol_stream_builder = protocol_stream
            .auth_key(Some(tycho_api_key))
            .skip_state_decode_failures(true)
            .set_tokens(token_addrs.clone());
        let (tx, rx) = watch::channel(None);

        let worker = Worker {
            chain: chain.clone(),
            protocol_stream_builder: Box::pin(protocol_stream_builder),
            block_sim_tx: tx,
            shutdown_token: shutdown_token.clone(),
        };

        Ok(Handle {
            chain,
            shutdown_token,
            worker_handle: Some(tokio::spawn(worker.run())),
            block_sim_tx: rx,
        })
    }

    fn add_exchanges_for_chain(
        chain: &Chain,
        protocol_stream: ProtocolStreamBuilder,
        tvl_filter: ComponentFilter,
    ) -> eyre::Result<ProtocolStreamBuilder> {
        match chain.name {
            tycho_common::models::Chain::Ethereum => Ok(protocol_stream
                .exchange::<UniswapV2State>("uniswap_v2", tvl_filter.clone(), None)
                .exchange::<UniswapV2State>("sushiswap_v2", tvl_filter.clone(), None)
                .exchange::<PancakeswapV2State>("pancakeswap_v2", tvl_filter.clone(), None)
                .exchange::<UniswapV3State>("uniswap_v3", tvl_filter.clone(), None)
                .exchange::<UniswapV3State>("pancakeswap_v3", tvl_filter.clone(), None)),
            tycho_common::models::Chain::Base => Ok(protocol_stream
                .exchange::<UniswapV2State>("uniswap_v2", tvl_filter.clone(), None)
                .exchange::<UniswapV3State>("uniswap_v3", tvl_filter.clone(), None)),
            tycho_common::models::Chain::Unichain => Ok(protocol_stream
                .exchange::<UniswapV2State>("uniswap_v2", tvl_filter.clone(), None)
                .exchange::<UniswapV3State>("uniswap_v3", tvl_filter.clone(), None)),
            _ => Err(eyre!("unsupported chain variant")),
        }
    }
}

pub struct Handle {
    chain: Chain,
    shutdown_token: CancellationToken,
    worker_handle: Option<tokio::task::JoinHandle<eyre::Result<()>>>,
    block_sim_tx: watch::Receiver<Option<BlockSim>>,
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
    pub fn get_block_sim_rx(&self) -> WatchStream<Option<BlockSim>> {
        WatchStream::from_changes(self.block_sim_tx.clone())
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
                Err(e) => Err(e),
            },
            Err(e) => Err(e).wrap_err("tycho collector task panicked"),
        })
    }
}

pub(super) struct Worker {
    chain: Chain,
    protocol_stream_builder: Pin<Box<dyn Future<Output = ProtocolStreamBuilder> + Send>>,
    block_sim_tx: watch::Sender<Option<BlockSim>>,
    shutdown_token: CancellationToken,
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
                        "Received block simulation from Tycho stream"
                    );
                    let block =
                        if let Some(old_block) = curr_block_sim.take() {
                            let new_block = old_block.apply_update(block_update).wrap_err("Failed to apply block update")?;
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
                        };
                    curr_block_sim = Some(block.clone());

                    match block_sim_tx.send(curr_block_sim.clone()) {
                        Err(SendError(Some(block_sim))) => {
                            error!(block_height = %block_sim.height, "Failed to collect block simulation update");
                            bail!("Failed to collect block simulation update")
                        }
                        Err(SendError(None)) => {
                            error!("Channel has no receivers. Failed to collect initial block simulation update");
                            bail!("Failed to collect initial block simulation update")
                        }
                        Ok(_) => {
                            debug!("Collected block simulation update");
                        }
                    }
                }
            }
        }
    }
}
