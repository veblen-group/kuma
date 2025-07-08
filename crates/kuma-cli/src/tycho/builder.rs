use std::{collections::HashSet, str::FromStr as _};
use num_bigint::BigUint;

use tycho_simulation::{
    evm::{
        protocol::{
            pancakeswap_v2::state::PancakeswapV2State, uniswap_v2::state::UniswapV2State,
            uniswap_v3::state::UniswapV3State,
        },
        stream::ProtocolStreamBuilder,
        tycho_models::Chain,
    },
    models::Token,
    tycho_client::feed::component_tracker::ComponentFilter,
};

use crate::chain::ChainInfo;
use super::Worker;

struct Builder {
    pub(crate) url: String,
    pub(crate) api_key: String,
    pub(crate) add_tvl_threshold: f64,
    pub(crate) remove_tvl_threshold: f64,
    pub(crate) no_tls: bool,
    pub(crate) chain_info: ChainInfo,
    pub(crate) tokens: HashSet<Token>,
}

impl Builder {
    pub fn build(self) -> super::Handle {
        let Self {
            url,
            add_tvl_threshold,
            remove_tvl_threshold,
            chain_info,
            api_key,
            ..
        } = self;

        // make protocol stream
        let chain = Chain::from_str(&chain_info.chain).expect("invalid chain id");
        let mut protocol_stream = ProtocolStreamBuilder::new(&url, chain);

        let tvl_filter = ComponentFilter::with_tvl_range(remove_tvl_threshold, add_tvl_threshold);
        // set up exchanges
        // this is for eth l1, will depend on the chain
        protocol_stream = protocol_stream
            .exchange::<UniswapV2State>("uniswap_v2", tvl_filter.clone(), None)
            .exchange::<UniswapV2State>("sushiswap_v2", tvl_filter.clone(), None)
            .exchange::<PancakeswapV2State>("pancakeswap_v2", tvl_filter.clone(), None)
            .exchange::<UniswapV3State>("uniswap_v3", tvl_filter.clone(), None)
            .exchange::<UniswapV3State>("pancakeswap_v3", tvl_filter.clone(), None);

        // TODO: get capacity from config?
        // let (tx, rx) = broadcast::channel(256);

        // TODO: Implement proper Handle creation
        // For now, create a placeholder Handle
        let handle = super::Handle::new(
            chain_info.clone(),
            tokio_util::sync::CancellationToken::new(),
            tokio::spawn(async { Ok(()) }),
            super::ChainSpecificAssetState {
                asset_a: Token { 
                    symbol: "A".to_string(), 
                    address: "0x1".as_bytes().to_vec().into(), 
                    decimals: 18,
                    gas: BigUint::from(0u64),
                },
                asset_b: Token { 
                    symbol: "B".to_string(), 
                    address: "0x2".as_bytes().to_vec().into(), 
                    decimals: 18,
                    gas: BigUint::from(0u64),
                },
                tx: tokio::sync::broadcast::channel(10).0,
                rx: tokio_stream::wrappers::BroadcastStream::new(tokio::sync::broadcast::channel(10).1),
            },
            super::ChainSpecificAssetState {
                asset_a: Token { 
                    symbol: "A".to_string(), 
                    address: "0x1".as_bytes().to_vec().into(), 
                    decimals: 18,
                    gas: BigUint::from(0u64),
                },
                asset_b: Token { 
                    symbol: "B".to_string(), 
                    address: "0x2".as_bytes().to_vec().into(), 
                    decimals: 18,
                    gas: BigUint::from(0u64),
                },
                tx: tokio::sync::broadcast::channel(10).0,
                rx: tokio_stream::wrappers::BroadcastStream::new(tokio::sync::broadcast::channel(10).1),
            },
        );

        handle
    }
}
