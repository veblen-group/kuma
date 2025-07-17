use std::collections::HashMap;

use color_eyre::eyre::{self, Context as _};
use futures::StreamExt as _;
use tracing::info;
use tycho_common::models::token::Token;

use crate::{
    Cli, Commands,
    chain::Chain,
    collector,
    config::Config,
    state::pair::Pair,
    utils::{get_chain_pairs, get_chains_from_cli, parse_chain_assets},
};

pub(crate) struct Kuma {
    #[allow(unused)]
    all_tokens: HashMap<Chain, HashMap<tycho_common::Bytes, Token>>,
    #[allow(unused)]
    pair: Pair,
    // slow chain
    #[allow(unused)]
    chain_a: Chain,
    // fast chain
    #[allow(unused)]
    chain_b: Chain,
    cli: Cli,
    collector_a_handle: crate::collector::Handle,
    collector_b_handle: crate::collector::Handle,
}

impl Kuma {
    pub fn spawn(cfg: Config, cli: Cli) -> eyre::Result<Self> {
        let Config {
            chains,
            tokens,
            tycho_api_key,
            add_tvl_threshold,
            remove_tvl_threshold,
            ..
        } = cfg;

        let chain_assets =
            parse_chain_assets(chains, tokens).expect("Failed to parse chain assets");

        info!("Parsed {} chains from config:", chain_assets.len());

        for (chain, _tokens) in &chain_assets {
            info!(chain.name = %chain.name,
                    chain.id = %chain.metadata.id(),
                    "🔗");
        }

        let mut pairs = get_chain_pairs(&cli.token_a, &cli.token_b, &chain_assets);
        let (chain_a, chain_b) = get_chains_from_cli(&cli, &chain_assets);

        let collector_a_handle = make_collector(
            chain_a.clone(),
            chain_assets[&chain_a].clone(),
            &tycho_api_key,
            add_tvl_threshold,
            remove_tvl_threshold,
        )
        .wrap_err("failed to start chain a collector")?;

        let collector_b_handle = make_collector(
            chain_b.clone(),
            chain_assets[&chain_b].clone(),
            &tycho_api_key,
            add_tvl_threshold,
            remove_tvl_threshold,
        )
        .wrap_err("failed to start chain a collector")?;

        Ok(Self {
            cli,
            chain_a: chain_a.clone(),
            chain_b,
            all_tokens: chain_assets,
            // TODO: do i have one pair or a pair per chain?
            pair: pairs.remove(&chain_a).expect("pair for chain a not found"),
            collector_a_handle,
            collector_b_handle,
        })
    }

    pub async fn run(self) -> eyre::Result<()> {
        let Self {
            cli,
            pair,
            collector_a_handle,
            collector_b_handle,
            ..
        } = self;

        if let Commands::GenerateSignals = cli.command {
            info!(command = "generate signals");

            let mut chain_a_pair_stream = collector_a_handle.get_pair_state_stream(&pair);
            let mut chain_b_pair_stream = collector_b_handle.get_pair_state_stream(&pair);

            // read state from stream
            let block_a = chain_a_pair_stream
                .next()
                .await
                .expect("chain a stream should yield initial block");
            let block_b = chain_b_pair_stream
                .next()
                .await
                .expect("chain b stream should yield initial block");

            info!(block = %block_a.block_number, "reaped initial block from chain a");
            info!(block = %block_b.block_number, "reaped initial block from chain b");

            // precompute data for signal

            // compute arb signal
            // log result
        }

        if let Commands::DryRun = cli.command {
            // set up tycho encoder
            // set up signer
        }

        if let Commands::Execute = cli.command {
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
