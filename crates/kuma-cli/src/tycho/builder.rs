use std::collections::HashMap;

use color_eyre::eyre::Context as _;
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

use crate::{chain::ChainInfo, tycho::Worker};

pub(crate) struct Builder {
    pub(crate) url: String,
    pub(crate) api_key: String,
    pub(crate) add_tvl_threshold: f64,
    pub(crate) remove_tvl_threshold: f64,
    pub(crate) chain_info: ChainInfo,
    pub(crate) tokens: HashMap<Bytes, Token>,
}

impl Builder {
    pub fn build(self) -> super::Handle {
        let Self {
            url,
            add_tvl_threshold,
            remove_tvl_threshold,
            chain_info,
            api_key,
            tokens,
            ..
        } = self;

        // make protocol stream
        let mut protocol_stream = ProtocolStreamBuilder::new(&url, chain_info.chain);

        let tvl_filter = ComponentFilter::with_tvl_range(remove_tvl_threshold, add_tvl_threshold);
        // set up exchanges
        // this is for eth l1, will depend on the chain
        protocol_stream = protocol_stream
            .exchange::<UniswapV2State>("uniswap_v2", tvl_filter.clone(), None)
            .exchange::<UniswapV2State>("sushiswap_v2", tvl_filter.clone(), None)
            .exchange::<PancakeswapV2State>("pancakeswap_v2", tvl_filter.clone(), None)
            .exchange::<UniswapV3State>("uniswap_v3", tvl_filter.clone(), None)
            .exchange::<UniswapV3State>("pancakeswap_v3", tvl_filter.clone(), None);

        let protocol_stream_builder = protocol_stream
            .auth_key(Some(api_key))
            .skip_state_decode_failures(true)
            .set_tokens(tokens.clone());

        // TODO: get capacity from config?
        // let (tx, rx) = broadcast::channel(256);
        // let handle = Handle::new();
        let worker_handle = tokio::task::spawn(async {
            // TODO: should  i move this into the worker?
            let worker = Worker {
                // TODO: do i really wanna get rid of these?
                // uri: Uri::from_str(&url).expect("invalid uri"),
                // api_key: api_key.clone(),
                protocol_stream_builder: Box::pin(protocol_stream_builder),
                tokens: tokens,
            };

            worker.run().await
        });

        let shutdown_token = CancellationToken::new();

        // TODO: make the state update streams

        super::Handle {
            chain_info,
            shutdown_token,
            worker_handle,
        }
    }
}
