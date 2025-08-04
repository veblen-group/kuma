use std::{
    fmt::{self, Display},
    str::FromStr,
};

use alloy_chains::{self, NamedChain};
use color_eyre::eyre::{self, Context, eyre};
use serde::{Deserialize, Serialize};
use tycho_common::models as tycho_models;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Chain {
    pub name: tycho_models::Chain,
    #[serde(skip)]
    pub metadata: alloy_chains::Chain,
    #[serde(skip)]
    pub rpc_url: String,
    #[serde(skip)]
    pub tycho_url: String,
}

impl Chain {
    pub fn new(name: &str, rpc_url: &str, tycho_url: &str) -> eyre::Result<Self> {
        let name = tycho_models::Chain::from_str(name)
            .wrap_err("failed to parse chain name into tycho::models::Chain")?;
        let metadata = match name {
            tycho_models::Chain::Ethereum => alloy_chains::Chain::from(NamedChain::Mainnet),
            tycho_models::Chain::Base => alloy_chains::Chain::from(NamedChain::Base),
            tycho_models::Chain::Unichain => alloy_chains::Chain::from(NamedChain::Unichain),
            _ => return Err(eyre!("unsupported chain {}", name)),
        };

        Ok(Self {
            name,
            metadata,
            rpc_url: rpc_url.to_string(),
            tycho_url: tycho_url.to_string(),
        })
    }

    #[allow(unused)]
    pub fn chain_id(&self) -> u64 {
        self.metadata.id()
    }

    #[cfg(test)]
    pub fn eth_mainnet() -> Self {
        Self {
            name: tycho_models::Chain::Ethereum,
            metadata: alloy_chains::Chain::from_named(NamedChain::Mainnet),
            rpc_url: "https://mainnet.infura.io/v3/".to_string(),
            tycho_url: "tycho-beta.propellerheads.xyz".to_string(),
        }
    }

    #[cfg(test)]
    pub fn base_mainnet() -> Self {
        Self {
            name: tycho_models::Chain::Base,
            metadata: alloy_chains::Chain::from_named(NamedChain::Base),
            rpc_url: "https://base-mainnet.infura.io/v3/".to_string(),
            tycho_url: "tycho-base-beta.propellerheads.xyz".to_string(),
        }
    }

    #[cfg(test)]
    #[allow(unused)]
    pub fn unichain_mainnet() -> Self {
        Self {
            name: tycho_models::Chain::Unichain,
            metadata: alloy_chains::Chain::from_named(NamedChain::Unichain),
            rpc_url: "https://unichain-mainnet.infura.io/v3/".to_string(),
            tycho_url: "tycho-unichain-beta.propellerheads.xyz".to_string(),
        }
    }
}

impl Display for Chain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (id={})", self.name, self.chain_id())
    }
}
