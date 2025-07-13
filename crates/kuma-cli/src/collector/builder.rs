use std::collections::HashMap;

use color_eyre::eyre::{self, Context as _, eyre};
use tokio_util::sync::CancellationToken;
use tycho_common::Bytes;
use tycho_simulation::{
    evm::{
        protocol::{
            pancakeswap_v2::state::PancakeswapV2State, uniswap_v2::state::UniswapV2State,
            uniswap_v3::state::UniswapV3State,
        },
        stream::ProtocolStreamBuilder,
    },
    models::Token,
    tycho_client::feed::component_tracker::ComponentFilter,
};

use super::Worker;
use crate::chain::Chain;

pub(crate) struct Builder {
    pub(crate) chain: Chain,
    pub(crate) tycho_url: String,
    pub(crate) api_key: String,
    pub(crate) tokens: HashMap<Bytes, Token>,
    pub(crate) add_tvl_threshold: f64,
    pub(crate) remove_tvl_threshold: f64,
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

        let (block_rx, block_tx) = watch::channel();

        let worker = Worker {
            // TODO: do i really wanna get rid of these?
            // uri: Uri::from_str(&url).expect("invalid uri"),
            // api_key: api_key.clone(),
            protocol_stream_builder: Box::pin(protocol_stream_builder),
            tokens: tokens,
            chain: chain.clone(),
            block_tx,
        };
        let worker_handle = tokio::task::spawn(async { worker.run().await });

        let shutdown_token = CancellationToken::new();

        // TODO: make the state update streams

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
