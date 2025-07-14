use std::{collections::HashMap, str::FromStr as _};

use color_eyre::eyre::{self, Context as _, OptionExt as _};
use tracing::{info, warn};
use tycho_simulation::{evm::tycho_models, models::Token};

use crate::{
    Cli,
    chain::Chain,
    collector,
    config::{ChainConfig, TokenConfig},
    state::pair::Pair,
};

pub(crate) fn parse_chain_assets(
    chains: Vec<ChainConfig>,
    tokens: HashMap<String, TokenConfig>,
) -> eyre::Result<HashMap<Chain, HashMap<tycho_common::Bytes, Token>>> {
    let chains = chains
        .iter()
        .map(
            |ChainConfig {
                 name,
                 rpc_url,
                 tycho_url,
             }| {
                Chain::new(&name, &rpc_url, &tycho_url).wrap_err("failed to parse chain info")
            },
        )
        .collect::<eyre::Result<Vec<Chain>>>()
        .expect("failed to parse chain configs");

    Ok(chains
        .into_iter()
        .map(|chain| {
            let tokens = tokens
                .iter()
                .map(|(symbol, token_config)| {
                    let addr = token_config
                        .addresses
                        .get(&chain.name)
                        .ok_or_eyre("token address for {symbol} on chain {chain.name} not found")?
                        .clone();

                    let token = Token::new(
                        &addr.to_string(),
                        token_config.decimals,
                        &symbol,
                        token_config.transfer_gas.into(),
                    );
                    Ok((addr, token))
                })
                .collect::<eyre::Result<HashMap<tycho_common::Bytes, Token>>>()?;
            Ok((chain, tokens))
        })
        .collect::<eyre::Result<HashMap<Chain, HashMap<tycho_common::Bytes, Token>>>>()?)
}

pub(crate) fn log_chain_tokens(chain_tokens: &HashMap<Chain, HashMap<tycho_common::Bytes, Token>>) {
    info!("Parsed {} chains from config:", chain_tokens.len());

    for (chain, _tokens) in chain_tokens {
        info!(chain.name = %chain.name,
            chain.id = %chain.metadata.id(),
            "🔗");
    }
}
pub(crate) fn get_chain_pairs(
    token_a: &str,
    token_b: &str,
    chain_tokens: &HashMap<Chain, HashMap<tycho_common::Bytes, Token>>,
) -> HashMap<Chain, Pair> {
    let mut pairs = HashMap::new();

    for (chain, tokens) in chain_tokens {
        let a = tokens
            .iter()
            .filter(|(_addr, token)| token.symbol == token_a.to_ascii_uppercase())
            .next();
        let b = tokens
            .iter()
            .filter(|(_addr, token)| token.symbol == token_b.to_ascii_uppercase())
            .next();

        match (a, b) {
            (None, _) | (_, None) => {
                warn!(pair.token_a = %token_a, pair.token_b = %token_b, chain.name = %chain.name, "🚫 Token pair not configured on chain");
            }
            (Some((_, a)), Some((_, b))) => {
                let pair = Pair::new(a.clone(), b.clone());
                pairs.insert(chain.clone(), pair);

                info!(pair.token_a = %token_a, pair.token_b = %token_b, chain.name = %chain.name, "🔄 Token pair configured");
            }
        }
    }

    pairs
}

pub(crate) fn get_chains_from_cli(
    cli: &Cli,
    chain_tokens: &HashMap<Chain, HashMap<tycho_common::Bytes, Token>>,
) -> (Chain, Chain) {
    let chain_a = chain_tokens
        .keys()
        .find(|chain| {
            chain.name == tycho_models::Chain::from_str(&cli.chain_a).expect("Invalid chain a name")
        })
        .expect("Chain A not configured")
        .clone();
    let chain_b = chain_tokens
        .keys()
        .find(|chain| {
            chain.name == tycho_models::Chain::from_str(&cli.chain_b).expect("Invalid chain b name")
        })
        .expect("Chain B not configured")
        .clone();

    (chain_a, chain_b)
}

pub(crate) fn make_collectors(
    chain_a: Chain,
    chain_b: Chain,
    chain_tokens: &HashMap<Chain, HashMap<tycho_common::Bytes, Token>>,
    tycho_api_key: &str,
    add_tvl_threshold: f64,
    remove_tvl_threshold: f64,
) -> eyre::Result<(collector::Handle, collector::Handle)> {
    let tokens_a = chain_tokens
        .get(&chain_a)
        .expect("No tokens found for base");
    let res_a = collector::Builder {
        tycho_url: chain_a.tycho_url.clone(),
        api_key: tycho_api_key.to_string(),
        add_tvl_threshold,
        remove_tvl_threshold,
        tokens: tokens_a.clone(),
        chain: chain_a.clone(),
    }
    .build();

    let tokens_b = chain_tokens
        .get(&chain_b)
        .expect("No tokens found for base");
    let res_b = collector::Builder {
        tycho_url: chain_b.tycho_url.clone(),
        api_key: tycho_api_key.to_string(),
        add_tvl_threshold,
        remove_tvl_threshold,
        tokens: tokens_b.clone(),
        chain: chain_b.clone(),
    }
    .build();

    // wait for startup
    Ok((
        match res_a {
            Ok(handle) => handle,
            Err(e) => Err(e).wrap_err("failed to start stream for chain a: {chain_a}")?,
        },
        match res_b {
            Ok(handle) => handle,
            Err(e) => Err(e).wrap_err("failed to start stream for chain b: {chain_b}")?,
        },
    ))
}
