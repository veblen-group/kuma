use std::{collections::HashMap, sync::Arc};

use color_eyre::eyre::{self, Context as _, eyre};
use tokio::sync::{broadcast, watch};
use tokio_util::sync::CancellationToken;
use tycho_simulation::{
    evm::{
        protocol::{
            pancakeswap_v2::state::PancakeswapV2State, uniswap_v2::state::UniswapV2State,
            uniswap_v3::state::UniswapV3State,
        },
        stream::ProtocolStreamBuilder,
    },
    tycho_client::feed::component_tracker::ComponentFilter,
    tycho_common::{self, Bytes, models::token::Token},
};

use crate::{
    chain::Chain,
    collector::{
        self,
        eth::{self, EthBlock},
    },
    state::tycho::BlockSim,
};

pub struct Builder {
    pub chain: Chain,
    pub tycho_url: String,
    pub api_key: String,
    pub tokens: HashMap<Bytes, Token>,
    pub add_tvl_threshold: f64,
    pub remove_tvl_threshold: f64,
    pub shutdown_token: CancellationToken,
    // TODO: take in provider
}

impl Builder {
    pub fn build(self) -> eyre::Result<super::Handle> {
        let Self {
            tycho_url: url,
            add_tvl_threshold,
            remove_tvl_threshold,
            chain,
            api_key,
            tokens,
            shutdown_token,
            ..
        } = self;

        // make protocol stream
        let protocol_stream = ProtocolStreamBuilder::new(&url, chain.name);
        let tvl_filter = ComponentFilter::with_tvl_range(remove_tvl_threshold, add_tvl_threshold);
        let protocol_stream = Self::add_exchanges_for_chain(&chain, protocol_stream, tvl_filter)
            .wrap_err("failed to set exchanges for {chain.name}.")?;

        let protocol_stream_builder = protocol_stream
            .auth_key(Some(api_key))
            .skip_state_decode_failures(true)
            .set_tokens(tokens.clone());

        let (eth_tx, eth_rx) = broadcast::channel::<EthBlock>(1);
        let (block_sim_tx, block_sim_rx) = broadcast::channel::<BlockSim>(1);

        let (block_tx, block_rx) = watch::channel::<Arc<Option<BlockSim>>>(Arc::new(None));

        let eth_worker = eth::Worker {
            shutdown_token: shutdown_token.clone(),
            chain,
            latest_block_tx: eth_tx,
            account_addr: todo!(),
            token_addrs: todo!(),
            ws_url: todo!(),
        };

        let tycho_worker = collector::tycho::Worker {
            protocol_stream_builder: Box::pin(protocol_stream_builder),
            chain: chain.clone(),
            block_sim_tx: block_sim_tx.clone(),
            shutdown_token: shutdown_token.clone(),
        };

        let worker = collector::Worker {
            // TODO: do i really wanna get rid of these or keep them for reconnect?
            // uri: Uri::from_str(&url).expect("invalid uri"),
            // api_key: api_key.clone(),
            chain: chain.clone(),
            block_tx: block_tx,
            shutdown_token: shutdown_token.clone(),
            account_addr: todo!(),
            token_addrs: todo!(),
            ws_url: todo!(),
        };
        let worker_handle = tokio::task::spawn(async { worker.run().await });

        Ok(super::Handle {
            chain,
            shutdown_token,
            worker_handle: Some(worker_handle),
            block_rx,
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
