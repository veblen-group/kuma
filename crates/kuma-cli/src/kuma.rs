use std::collections::HashMap;

use color_eyre::eyre::{self, Context as _, ensure};
use futures::StreamExt as _;
use num_bigint::BigUint;
use tracing::info;
use tycho_common::models::token::Token;

use crate::{
    Cli, Commands,
    chain::Chain,
    collector,
    config::Config,
    state::pair::Pair,
    strategy::CrossChainSingleHop,
    utils::{get_chain_pairs, get_chains_from_cli, parse_chain_assets},
};

pub(crate) struct Kuma {
    command: Commands,

    #[allow(unused)]
    all_tokens: HashMap<Chain, HashMap<tycho_common::Bytes, Token>>,
    slow_pair: Pair,
    slow_chain: Chain,
    fast_pair: Pair,
    fast_chain: Chain,

    slow_collector_handle: crate::collector::Handle,
    fast_collector_handle: crate::collector::Handle,
    strategy: CrossChainSingleHop,
}

impl Kuma {
    pub fn spawn(cfg: Config, cli: Cli) -> eyre::Result<Self> {
        let Config {
            chains,
            tokens,
            tycho_api_key,
            add_tvl_threshold,
            remove_tvl_threshold,
            max_slippage_bps,
            congestion_risk_discount_bps,
            binary_search_steps,
            max_trade_size,
            ..
        } = cfg;

        let chain_assets =
            parse_chain_assets(chains, tokens).expect("Failed to parse chain assets");

        info!("Parsed {} chains from config:", chain_assets.len());

        for (chain, _tokens) in &chain_assets {
            info!(chain.name = %chain.name,
                    chain.id = %chain.metadata.id(),
                    "🔗 Initialized chain info from config");
        }

        let pairs = get_chain_pairs(&cli.token_a, &cli.token_b, &chain_assets);
        let (slow_chain, fast_chain) = get_chains_from_cli(&cli, &chain_assets);
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
            chain_assets[&slow_chain].clone(),
            &tycho_api_key,
            add_tvl_threshold,
            remove_tvl_threshold,
        )
        .wrap_err("failed to start chain a collector")?;

        let fast_collector_handle = make_collector(
            fast_chain.clone(),
            chain_assets[&fast_chain].clone(),
            &tycho_api_key,
            add_tvl_threshold,
            remove_tvl_threshold,
        )
        .wrap_err("failed to start chain a collector")?;

        // initialize single hop strategy
        let strategy = CrossChainSingleHop {
            slow_pair: slow_pair.clone(),
            fast_pair: fast_pair.clone(),
            // TODO: ??
            min_profit_threshold: 0.5,
            // TODO: make token -> chain -> bigint inventory map from config
            available_inventory: BigUint::from(max_trade_size),
            binary_search_steps,
            max_slippage_bps: (max_slippage_bps as i32)
                .try_into()
                .expect("Failed to convert max_slippage_bps to f64"),
            congestion_risk_discount_bps,
        };

        Ok(Self {
            command: cli.command,
            all_tokens: chain_assets,
            slow_chain,
            slow_pair: slow_pair.clone(),
            fast_chain,
            fast_pair: fast_pair.clone(),
            slow_collector_handle,
            fast_collector_handle,
            strategy,
        })
    }

    pub async fn run(self) -> eyre::Result<()> {
        let Self {
            command,
            slow_chain,
            slow_pair,
            fast_chain,
            fast_pair,
            slow_collector_handle,
            fast_collector_handle,
            strategy,
            ..
        } = self;

        let _signal = {
            // TODO: do i need Commands::GenerateSignal if this is always run?
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

            info!(block = %slow_state.block_number, "reaped initial block from chain a");
            info!(block = %fast_state.block_number, "reaped initial block from chain b");

            // precompute data for signal
            let precompute = strategy.precompute(&slow_state, &slow_chain);

            // compute arb signal
            if let Some(signal) = strategy.generate_signal(&precompute, &fast_state, &fast_chain) {
                // TODO: display impl for signal
                info!(signal = ?signal,"generated signal");
                Some(signal)
            } else {
                info!("no signal generated");
                None
            }
        };

        if let Commands::DryRun = command {
            // set up tycho encoder
            // set up signer
        }

        if let Commands::Execute = command {
            // set up submission stuff
        }

        Ok(())
    }
}

pub(crate) fn make_collector(
    chain: Chain,
    tokens: HashMap<tycho_common::Bytes, Token>,
    tycho_api_key: &str,
    add_tvl_threshold: f64,
    remove_tvl_threshold: f64,
) -> eyre::Result<collector::Handle> {
    let handle = collector::Builder {
        tycho_url: chain.tycho_url.clone(),
        api_key: tycho_api_key.to_string(),
        add_tvl_threshold,
        remove_tvl_threshold,
        tokens,
        chain,
    }
    .build();

    handle.wrap_err("failed to start tycho collector for chain : {chain}")
}
