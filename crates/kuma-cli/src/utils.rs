use std::{collections::HashMap, str::FromStr as _};

use color_eyre::eyre::{self, Context as _, OptionExt as _};
use tracing::{info, warn};
use tycho_common::models::token::Token;
use tycho_simulation::evm::tycho_models;

use crate::{
    Cli,
    chain::Chain,
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
                        &addr,
                        &symbol,
                        token_config.decimals,
                        token_config.tax,
                        &token_config.gas.clone(),
                        chain.name,
                        token_config.quality,
                    );
                    Ok((addr, token))
                })
                .collect::<eyre::Result<HashMap<tycho_common::Bytes, Token>>>()?;
            Ok((chain, tokens))
        })
        .collect::<eyre::Result<HashMap<Chain, HashMap<tycho_common::Bytes, Token>>>>()?)
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
                warn!(pair.token_a = %token_a, pair.token_b = %token_b, chain.name = %chain.name, "Failed to initialized token pair for chain");
            }
            (Some((_, a)), Some((_, b))) => {
                let pair = Pair::new(a.clone(), b.clone());
                pairs.insert(chain.clone(), pair);

                info!(pair.token_a = %token_a, pair.token_b = %token_b, chain.name = %chain.name, "🪙 Successfully initialized token pair for chain");
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
