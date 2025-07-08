use figment::{
    Figment,
    providers::{Env, Format, Yaml},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tycho_common::{Bytes, models::Chain};

// TODO: add log level from env
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Token configurations
    pub tokens: HashMap<String, TokenConfig>,

    /// Chain configurations (deserialized from string map to Chain map)
    pub chains: Vec<ChainConfig>,

    /// Congestion risk discount factor (0.0 - 1.0)
    pub congestion_risk_discount: f64,

    /// Maximum acceptable slippage percentage
    pub max_slippage: f64,

    /// Private key bytes for transaction signing
    // TODO: replace with private key path
    pub private_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConfig {
    /// Token addresses on different chains
    pub addresses: HashMap<Chain, Bytes>,

    /// Token decimals
    pub decimals: usize,

    /// Amount of gas to use for transfers
    pub transfer_gas: u64,

    /// Existing inventory for this token
    pub inventory: f64,
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
            .merge(Yaml::file("Config.yaml"))
            .merge(Env::prefixed("KUMA_CLI_"))
            .extract()?;

        Ok(config)
    }
}
