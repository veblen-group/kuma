use std::{collections::HashMap, str::FromStr as _};

use color_eyre::eyre::{self, Context as _};
use futures::StreamExt as _;
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument};
use tycho_simulation::tycho_common::{self, models::token::Token};

use core::{
    chain::Chain,
    collector,
    config::Config,
    signals,
    state::pair::Pair,
    strategy::{self, CrossChainSingleHop},
};

use crate::cli::StrategyArgs;

pub(crate) struct Kuma {
    #[allow(unused)]
    all_tokens: HashMap<Chain, HashMap<tycho_common::Bytes, Token>>,

    strategy: CrossChainSingleHop,

    slow_pair: Pair,
    slow_chain: Chain,
    slow_block_handle: collector::Handle,
    #[allow(unused)]
    slow_eth_handle: collector::eth::Handle,
    #[allow(unused)]
    slow_tycho_handle: collector::tycho::Handle,

    fast_pair: Pair,
    fast_chain: Chain,
    fast_block_handle: collector::Handle,
    #[allow(unused)]
    fast_eth_handle: collector::eth::Handle,
    #[allow(unused)]
    fast_tycho_handle: collector::tycho::Handle,
}

impl Kuma {
    pub fn spawn(
        cfg: Config,
        strategy_config: StrategyArgs,
        shutdown_token: CancellationToken,
    ) -> eyre::Result<Self> {
        let (tokens_by_chain, inventory) = cfg
            .build_addrs_and_inventory()
            .expect("Failed to parse chain assets");

        info!("Parsed {} chains from config:", tokens_by_chain.len());

        for chain in tokens_by_chain.keys() {
            info!(chain.name = %chain.name,
                        chain.id = %chain.metadata.id(),
                        "🔗 Initialized chain info from config");
        }

        let pairs = Config::get_chain_pairs(
            &strategy_config.token_a,
            &strategy_config.token_b,
            &inventory,
        );

        let Config {
            tycho_api_key,
            add_tvl_threshold,
            remove_tvl_threshold,
            max_slippage_bps,
            congestion_risk_discount_bps,
            binary_search_steps,
            ..
        } = cfg;

        let (slow_chain, fast_chain) = get_chains_from_names(
            strategy_config.slow_chain,
            strategy_config.fast_chain,
            &tokens_by_chain,
        );
        //
        // set up tycho stream collectors
        let (slow_block_handle, slow_eth_handle, slow_tycho_handle) = collector::Builder {
            tycho_url: slow_chain.tycho_url.clone(),
            tycho_api_key: tycho_api_key.to_string(),
            add_tvl_threshold,
            remove_tvl_threshold,
            token_addrs: tokens_by_chain[&slow_chain].clone(),
            chain: slow_chain.clone(),
            shutdown_token: shutdown_token.clone(),
        }
        .build()
        .wrap_err("failed to start tycho collector for chain : {slow_chain}")?;

        let (fast_block_handle, fast_eth_handle, fast_tycho_handle) = collector::Builder {
            tycho_url: fast_chain.tycho_url.clone(),
            tycho_api_key: tycho_api_key.to_string(),
            add_tvl_threshold,
            remove_tvl_threshold,
            token_addrs: tokens_by_chain[&fast_chain].clone(),
            chain: fast_chain.clone(),
            shutdown_token: shutdown_token.clone(),
        }
        .build()
        .wrap_err("failed to start tycho collector for chain : {fast_chain}")?;

        let slow_pair = pairs.get(&slow_chain).expect(&format!(
            "could not find pair info for {:}",
            slow_chain.name
        ));

        let strategy = strategy::Builder {
            token_a: slow_pair.token_a().symbol.clone(),
            token_b: slow_pair.token_b().symbol.clone(),
            slow_chain: slow_chain.clone(),
            fast_chain: fast_chain.clone(),
            inventory,
            binary_search_steps,
            max_slippage_bps,
            congestion_risk_discount_bps,
        }
        .build()
        .wrap_err("failed to build strategy")?;

        Ok(Self {
            all_tokens: tokens_by_chain,
            slow_chain,
            slow_pair: slow_pair.clone(),
            fast_chain,
            fast_pair: strategy.fast_pair.clone(),
            slow_block_handle,
            slow_eth_handle,
            slow_tycho_handle,
            fast_block_handle,
            fast_eth_handle,
            fast_tycho_handle,
            strategy,
        })
    }

    #[instrument(skip(self))]
    pub async fn generate_signal(self) -> eyre::Result<signals::CrossChainSingleHop> {
        let Self {
            slow_chain,
            slow_pair,
            fast_chain,
            fast_pair,
            slow_block_handle,
            fast_block_handle,
            strategy,
            ..
        } = self;

        info!(command = "generating signal");

        let mut slow_chain_states =
            slow_block_handle.get_block_state_stream(slow_pair.clone(), strategy.slow_usdc.clone());
        let mut fast_chain_states =
            fast_block_handle.get_block_state_stream(fast_pair.clone(), strategy.fast_usdc.clone());
        // read state from stream
        let slow_state = slow_chain_states
            .next()
            .await
            .expect("chain a stream should yield initial block");
        let fast_state = fast_chain_states
            .next()
            .await
            .expect("chain b stream should yield initial block");

        info!(block = %slow_state.pair_state.block_height, chain = %slow_chain.name, "reaped initial block");
        info!(block = %fast_state.pair_state.block_height, chain = %fast_chain.name, "reaped initial block");

        // precompute data for signal
        let precompute = strategy.precompute(slow_state.pair_state);

        info!(block_height = %precompute.block_height, chain = %slow_chain.name, "✅ precomputed data");
        let fast_sorted_spot_prices =
            core::strategy::simulation::make_sorted_spot_prices(&fast_state.pair_state, &fast_pair);
        // compute arb signal
        let signal = strategy.generate_signal(
            &precompute,
            fast_state.pair_state,
            fast_sorted_spot_prices,
        )?;

        info!(signal = ?signal, "📊 generated signal");

        Ok(signal)
    }
}

pub(crate) fn get_chains_from_names(
    slow_chain: String,
    fast_chain: String,
    chain_tokens: &HashMap<Chain, HashMap<tycho_common::Bytes, Token>>,
) -> (Chain, Chain) {
    let slow_chain = chain_tokens
        .keys()
        .find(|chain| {
            chain.name
                == tycho_common::models::Chain::from_str(&slow_chain)
                    .expect("Invalid slow chain name")
        })
        .expect("Chain A not configured")
        .clone();
    let fast_chain = chain_tokens
        .keys()
        .find(|chain| {
            chain.name
                == tycho_common::models::Chain::from_str(&fast_chain)
                    .expect("Invalid fast chain name")
        })
        .expect("Chain B not configured")
        .clone();

    (slow_chain, fast_chain)
}
