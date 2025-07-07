use crate::chain::ChainInfo;
use figment::{
    Figment,
    providers::{Env, Format, Yaml},
};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::str::FromStr as _;
use tycho_common::{Bytes, models::Chain};

// TODO: add log level from env
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Token configurations
    // TODO: this should be a HashMap<Bytes, Token> or something like that
    pub tokens: HashMap<String, TokenConfig>,

    /// Chain configurations (deserialized from string map to Chain map)
    #[serde(deserialize_with = "deserialize_chains")]
    pub chains: HashMap<Chain, ChainInfo>,

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
    #[serde(deserialize_with = "deserialize_token_addresses")]
    pub addresses: HashMap<Chain, Bytes>,

    /// Token decimals
    pub decimals: usize,

    /// Token symbol
    pub symbol: String,

    /// Amount of gas to use for transfers
    pub transfer_gas: u64,

    /// Existing inventory for this token
    pub inventory: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    /// RPC endpoint URL
    pub rpc_url: String,

    /// Chain ID
    pub chain_id: u64,

    /// Block time in milliseconds
    pub block_time_ms: u64,
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

fn deserialize_chains<'de, D>(deserializer: D) -> Result<HashMap<Chain, ChainInfo>, D::Error>
where
    D: Deserializer<'de>,
{
    let chains_map: HashMap<String, ChainConfig> = HashMap::deserialize(deserializer)?;

    let chains = chains_map
        .into_iter()
        .map(|(name, config)| {
            let chain = Chain::from_str(&name).map_err(serde::de::Error::custom)?;
            let chain_info = ChainInfo {
                chain: chain.clone(),
                chain_id: config.chain_id,
                block_time: config.block_time_ms,
                rpc_url: config.rpc_url,
            };
            Ok((chain, chain_info))
        })
        .collect::<Result<HashMap<_, _>, _>>()?;

    Ok(chains)
}

fn deserialize_token_addresses<'de, D>(deserializer: D) -> Result<HashMap<Chain, Bytes>, D::Error>
where
    D: Deserializer<'de>,
{
    let addresses_map: HashMap<String, String> = HashMap::deserialize(deserializer)?;

    let addresses = addresses_map
        .into_iter()
        .map(|(chain_name, address)| {
            let chain = Chain::from_str(&chain_name).map_err(serde::de::Error::custom)?;
            Ok((chain, address.as_bytes().into()))
        })
        .collect::<Result<HashMap<_, _>, _>>()?;

    Ok(addresses)
}
