use color_eyre::eyre::{self, Context as _, OptionExt as _, eyre};
use figment::{
    Figment,
    providers::{Env, Format as _, Yaml},
};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};
use tracing::{info, warn};
use tycho_common::{Bytes, models::token::Token};

use crate::{chain::Chain, state::pair::Pair};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Database configuration
    pub database: DatabaseConfig,

    /// Server configuration
    pub server: ServerConfig,

    /// Arbitrage paths to create strategies for
    pub strategies: Vec<StrategyConfig>,

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

pub type AddressForToken = HashMap<tycho_common::Bytes, Token>;
pub type TokenAddressesForChain = HashMap<Chain, AddressForToken>;

pub type InventoryForToken = HashMap<Token, BigUint>;
pub type InventoriesForChain = HashMap<Chain, InventoryForToken>;

pub type PairForChain = HashMap<Chain, Pair>;

impl Config {
    /// Load configuration from environment and optional config file
    pub fn load() -> Result<Self, figment::Error> {
        let config: Config = Figment::new()
            .merge(Yaml::file("kuma.yaml"))
            .merge(Env::prefixed("KUMA_CLI_"))
            .extract()?;

        Ok(config)
    }

    /// Parse chain assets from the config, returning tokens and their inventories by chain
    pub fn build_addrs_and_inventory(
        &self,
    ) -> eyre::Result<(TokenAddressesForChain, InventoriesForChain)> {
        let chains = self
            .chains
            .iter()
            .map(
                |ChainConfig {
                     name,
                     rpc_url,
                     tycho_url,
                 }| {
                    Chain::new(name, rpc_url, tycho_url).wrap_err("failed to parse chain info")
                },
            )
            .collect::<eyre::Result<Vec<Chain>>>()
            .expect("failed to parse chain configs");

        let mut inventories_by_chain: HashMap<Chain, HashMap<Token, BigUint>> = HashMap::new();

        let mut tokens_by_chain = HashMap::new();
        for chain in chains {
            let mut tokens = HashMap::new();
            for (symbol, token_config) in &self.tokens {
                let addr = token_config
                    .addresses
                    .get(&chain.name)
                    .ok_or_eyre("token address for {symbol} on chain {chain.name} not found")?
                    .clone();

                let token = Token::new(
                    &addr,
                    symbol,
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

    /// Get trading pairs for given token symbols across configured chains
    pub fn get_chain_pairs(
        token_a: &str,
        token_b: &str,
        tokens_for_chain: &InventoriesForChain,
    ) -> PairForChain {
        let mut pairs = HashMap::new();

        for (chain, tokens) in tokens_for_chain {
            let a = tokens
                .keys()
                .find(|token| token.symbol.to_ascii_uppercase() == token_a.to_ascii_uppercase());
            let b = tokens
                .keys()
                .find(|token| token.symbol.to_ascii_uppercase() == token_b.to_ascii_uppercase());

            match (a, b) {
                (None, _) | (_, None) => {
                    warn!(a.expected = %token_a, a.parsed = %a.is_some(), b.expected = %token_b, b.parsed = %b.is_some(), chain = %chain.name, "Failed to initialize token pair for chain");
                }
                (Some(a), Some(b)) => {
                    let pair = Pair::new(a.clone(), b.clone());
                    pairs.insert(chain.clone(), pair);

                    info!(%token_a, %token_b, chain = %chain.name, "🪙 Successfully initialized token pair for chain");
                }
            }
        }

        pairs
    }
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub token_a: String,
    pub token_b: String,
    pub slow_chain: String,
    pub fast_chain: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub connection_timeout_secs: u64,
    pub idle_timeout_secs: u64,
}

impl DatabaseConfig {
    pub fn connection_timeout(&self) -> Duration {
        Duration::from_secs(self.connection_timeout_secs)
    }

    pub fn idle_timeout(&self) -> Duration {
        Duration::from_secs(self.idle_timeout_secs)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}
