use std::{collections::HashMap, fs, str::FromStr as _};

use color_eyre::eyre::{self, Context as _};
use futures::StreamExt as _;
use tracing::{info, instrument};
use tycho_common::models::token::Token;

use crate::{Cli, Commands, tokens::load_all_tokens};

use core::{
    chain::Chain, collector, config::Config, state::pair::Pair, strategy::CrossChainSingleHop,
};

pub(crate) struct Kuma {
    command: Commands,

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
    pub fn spawn(cfg: Config, cli: Cli) -> eyre::Result<Self> {
        let (tokens_by_chain, inventory) = cfg
            .parse_chain_assets()
            .expect("Failed to parse chain assets");

        info!("Parsed {} chains from config:", tokens_by_chain.len());

        for (chain, _tokens) in &tokens_by_chain {
            info!(chain.name = %chain.name,
                        chain.id = %chain.metadata.id(),
                        "🔗 Initialized chain info from config");
        }

        let pairs = cfg.get_chain_pairs(&cli.token_a, &cli.token_b)?;

        let Config {
            tycho_api_key,
            add_tvl_threshold,
            remove_tvl_threshold,
            max_slippage_bps,
            congestion_risk_discount_bps,
            binary_search_steps,
            ..
        } = cfg;

        let (slow_chain, fast_chain) = get_chains_from_cli(&cli, &tokens_by_chain);
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
        )
        .wrap_err("failed to start chain a collector")?;

        let fast_collector_handle = make_collector(
            fast_chain.clone(),
            tokens_by_chain[&fast_chain].clone(),
            &tycho_api_key,
            add_tvl_threshold,
            remove_tvl_threshold,
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
            command: cli.command,
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

        if let Commands::Tokens = command {
            let slow_chain_token_addrs = load_all_tokens(
                &slow_chain.tycho_url,
                false,
                Some("sampletoken"),
                slow_chain.name,
                Some(95),
                Some(7),
            )
            .await;
            let slow_chain_json = serde_json::to_string_pretty(&slow_chain_token_addrs)
                .expect("implements serde::Serialize");
            let slow_chain_file_name = format!("tokens.{}.json", slow_chain.name);
            fs::write(slow_chain_file_name, slow_chain_json)
                .wrap_err("failed to save slow chain tokens json")?;
            info!("loaded slow chain tokens");

            let fast_chain_token_addrs = load_all_tokens(
                &fast_chain.tycho_url,
                false,
                Some("sampletoken"),
                fast_chain.name,
                Some(95),
                Some(7),
            )
            .await;
            let fast_chain_json = serde_json::to_string_pretty(&fast_chain_token_addrs)
                .expect("implements serde::Serialize");
            let fast_chain_file_name = format!("tokens.{}.json", fast_chain.name);
            fs::write(fast_chain_file_name, fast_chain_json)
                .wrap_err("failed to save fast chain tokens json")?;
            info!("loaded fast chain tokens");

            return Ok(());
        }

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

        info!(block = %slow_state.block_height, chain = %slow_chain.name, "reaped initial block");
        info!(block = %fast_state.block_height, chain = %fast_chain.name, "reaped initial block");

        // precompute data for signal
        let precompute = strategy.precompute(slow_state);

        info!(block_height = %precompute.block_height, chain = %slow_chain.name, "✅ precomputed data");

        // compute arb signal
        let signal = strategy.generate_signal(precompute, fast_state)?;

        info!(signal = ?signal, "📊 generated signal");

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

pub(crate) fn get_chains_from_cli(
    cli: &Cli,
    chain_tokens: &HashMap<Chain, HashMap<tycho_common::Bytes, Token>>,
) -> (Chain, Chain) {
    let chain_a = chain_tokens
        .keys()
        .find(|chain| {
            chain.name
                == tycho_common::models::Chain::from_str(&cli.chain_a)
                    .expect("Invalid chain a name")
        })
        .expect("Chain A not configured")
        .clone();
    let chain_b = chain_tokens
        .keys()
        .find(|chain| {
            chain.name
                == tycho_common::models::Chain::from_str(&cli.chain_b)
                    .expect("Invalid chain b name")
        })
        .expect("Chain B not configured")
        .clone();

    (chain_a, chain_b)
}
