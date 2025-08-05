use std::{collections::HashMap, str::FromStr as _};

use color_eyre::eyre::{self, Context as _};
use futures::StreamExt as _;
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument};
use tycho_common::models::token::Token;

use core::{
    chain::Chain,
    collector,
    config::{Config, StrategyConfig},
    state::pair::Pair,
    strategy::CrossChainSingleHop,
};

pub(crate) struct Kuma {
    #[allow(unused)]
    all_tokens: HashMap<Chain, HashMap<tycho_common::Bytes, Token>>,
    slow_pair: Pair,
    slow_chain: Chain,
    fast_pair: Pair,
    fast_chain: Chain,

    slow_collector_handle: collector::Handle,
    fast_collector_handle: collector::Handle,
    strategy: CrossChainSingleHop,
}

impl Kuma {
    pub fn spawn(
        cfg: Config,
        strategy_config: StrategyConfig,
        shutdown_token: CancellationToken,
    ) -> eyre::Result<Self> {
        let (tokens_by_chain, inventory) = cfg
            .build_addrs_and_inventory()
            .expect("Failed to parse chain assets");

        info!("Parsed {} chains from config:", tokens_by_chain.len());

        for (chain, _tokens) in &tokens_by_chain {
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

        let (slow_chain, fast_chain) = get_chains_from_cli(
            strategy_config.slow_chain,
            strategy_config.fast_chain,
            &tokens_by_chain,
        );
        let slow_pair = pairs.get(&slow_chain).expect(&format!(
            "could not find pair info for {:}",
            slow_chain.name
        ));
        let fast_pair = pairs.get(&fast_chain).expect(&format!(
            "could not find pair info for {:}",
            fast_chain.name
        ));

        // set up tycho stream collectors
        let slow_collector_handle = make_collector(
            slow_chain.clone(),
            tokens_by_chain[&slow_chain].clone(),
            &tycho_api_key,
            add_tvl_threshold,
            remove_tvl_threshold,
            shutdown_token.clone(),
        )
        .wrap_err("failed to start chain a collector")?;

        let fast_collector_handle = make_collector(
            fast_chain.clone(),
            tokens_by_chain[&fast_chain].clone(),
            &tycho_api_key,
            add_tvl_threshold,
            remove_tvl_threshold,
            shutdown_token.clone(),
        )
        .wrap_err("failed to start chain a collector")?;

        // initialize single hop strategy
        let slow_inventory = (
            inventory[&slow_chain][slow_pair.token_a()].clone(),
            inventory[&slow_chain][slow_pair.token_b()].clone(),
        );
        let fast_inventory = (
            inventory[&fast_chain][fast_pair.token_a()].clone(),
            inventory[&fast_chain][fast_pair.token_b()].clone(),
        );

        let strategy = CrossChainSingleHop {
            slow_pair: slow_pair.clone(),
            slow_chain: slow_chain.clone(),
            fast_pair: fast_pair.clone(),
            fast_chain: fast_chain.clone(),
            slow_inventory,
            fast_inventory,
            binary_search_steps,
            max_slippage_bps,
            congestion_risk_discount_bps,
        };

        Ok(Self {
            all_tokens: tokens_by_chain,
            slow_chain,
            slow_pair: slow_pair.clone(),
            fast_chain,
            fast_pair: fast_pair.clone(),
            slow_collector_handle,
            fast_collector_handle,
            strategy,
        })
    }

    #[instrument(skip(self))]
    pub async fn generate_signal(self) -> eyre::Result<()> {
        let Self {
            slow_chain,
            slow_pair,
            fast_chain,
            fast_pair,
            slow_collector_handle,
            fast_collector_handle,
            strategy,
            ..
        } = self;

        info!(command = "generating signal");

        let mut slow_chain_states = slow_collector_handle.get_pair_state_stream(&slow_pair);
        let mut fast_chain_states = fast_collector_handle.get_pair_state_stream(&fast_pair);
        // read state from stream
        let slow_state = slow_chain_states
            .next()
            .await
            .expect("chain a stream should yield initial block");
        let fast_state = fast_chain_states
            .next()
            .await
            .expect("chain b stream should yield initial block");

        info!(block = %slow_state.block_height, chain = %slow_chain.name, "reaped initial block");
        info!(block = %fast_state.block_height, chain = %fast_chain.name, "reaped initial block");

        // precompute data for signal
        let precompute = strategy.precompute(slow_state);

        info!(block_height = %precompute.block_height, chain = %slow_chain.name, "✅ precomputed data");

        // compute arb signal
        let signal = strategy.generate_signal(&precompute, fast_state)?;

        info!(signal = ?signal, "📊 generated signal");

        Ok(())
    }
}

pub(crate) fn make_collector(
    chain: Chain,
    tokens: HashMap<tycho_common::Bytes, Token>,
    tycho_api_key: &str,
    add_tvl_threshold: f64,
    remove_tvl_threshold: f64,
    shutdown_token: CancellationToken,
) -> eyre::Result<collector::Handle> {
    let handle = collector::Builder {
        tycho_url: chain.tycho_url.clone(),
        api_key: tycho_api_key.to_string(),
        add_tvl_threshold,
        remove_tvl_threshold,
        tokens,
        chain,
        shutdown_token,
    }
    .build();

    handle.wrap_err("failed to start tycho collector for chain : {chain}")
}

pub(crate) fn get_chains_from_cli(
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
