use color_eyre::eyre::{self, Context as _, OptionExt as _, eyre};
use figment::{
    Figment,
    providers::{Env, Format, Yaml},
};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};
use tycho_common::{Bytes, models::token::Token};

use crate::{chain::Chain, state::pair::Pair};

// TODO: add log level from env
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Token configurations
    pub tokens: HashMap<String, TokenConfig>,

    /// Chain configurations (deserialized from string map to Chain map)
    pub chains: Vec<ChainConfig>,

    /// API key for Tycho Indexer
    pub tycho_api_key: String,

    /// Threshold for adding TVL to the system
    pub add_tvl_threshold: f64,

    /// Threshold for removing TVL from the system
    pub remove_tvl_threshold: f64,

    /// Congestion risk discount factor (0.0 - 1.0)
    pub congestion_risk_discount_bps: u64,

    /// Maximum acceptable slippage percentage
    pub max_slippage_bps: u64,

    pub binary_search_steps: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConfig {
    /// Token addresses on different chains
    pub addresses: HashMap<tycho_common::models::Chain, Bytes>,

    /// Token decimals
    pub decimals: u32,

    /// Taxs
    pub tax: u64,

    /// Amount of gas to use for transfers
    pub gas: Vec<Option<u64>>,

    /// Quality of the token
    pub quality: u32,

    /// Existing inventory for this token
    pub inventory: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    /// Chain name
    pub name: String,

    /// RPC endpoint URL
    pub rpc_url: String,

    /// RPC endpoint URL for Tycho Indexer
    pub tycho_url: String,
}

impl Config {
    /// Load configuration from environment and optional config file
    pub fn load() -> Result<Self, figment::Error> {
        let config: Config = Figment::new()
            .merge(Yaml::file("kuma.yaml"))
            .merge(Env::prefixed("KUMA_CLI_"))
            .extract()?;

        Ok(config)
    }
}

pub fn parse_chain_assets(
    chain_configs: Vec<ChainConfig>,
    token_configs: HashMap<String, TokenConfig>,
) -> eyre::Result<(
    HashMap<Chain, HashMap<tycho_common::Bytes, Token>>,
    HashMap<Chain, HashMap<Token, BigUint>>,
)> {
    let chains = chain_configs
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

    let mut inventories_by_chain: HashMap<Chain, HashMap<Token, BigUint>> = HashMap::new();

    let mut tokens_by_chain = HashMap::new();
    for chain in chains {
        let mut tokens = HashMap::new();
        for (symbol, token_config) in &token_configs {
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
                chain.name.clone(),
                token_config.quality,
            );

            let token_inventory =
                BigUint::from(token_config.inventory) * 10u128.pow(token.decimals);

            if let Some(token_inventories) = inventories_by_chain.get_mut(&chain) {
                match token_inventories.insert(token.clone(), token_inventory) {
                    Some(_) => return Err(eyre!("duplicate token inventory")),
                    None => (),
                }
            } else {
                inventories_by_chain.insert(
                    chain.clone(),
                    HashMap::from([(token.clone(), token_inventory)]),
                );
            }

            tokens.insert(addr, token);
        }
        tokens_by_chain.insert(chain, tokens);
    }

    Ok((tokens_by_chain, inventories_by_chain))
}

pub fn get_chain_pairs(
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
