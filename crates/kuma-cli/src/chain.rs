use std::str::FromStr;

use alloy_chains::{self, NamedChain};
use color_eyre::eyre::{self, Context, eyre};
use serde::{Deserialize, Serialize};
use tycho_common::models as tycho_models;

// TODO: impl Eq with just the name for simplicity?
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(crate) struct Chain {
    pub(crate) name: tycho_models::Chain,
    pub(crate) metadata: alloy_chains::Chain,
    pub(crate) rpc_url: String,
    pub(crate) tycho_url: String,
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

    pub fn chain_id(&self) -> u64 {
        self.metadata.id()
    }

    #[cfg(test)]
    pub fn eth_mainnet() -> Self {
        Self {
            name: tycho_models::Chain::Ethereum,
            metadata: alloy_chains::Chain::from_named(NamedChain::Mainnet),
            rpc_url: "https://mainnet.infura.io/v3/".to_string(),
            tycho_url: "https://eth.tycho.example.com".to_string(),
        }
    }

    #[cfg(test)]
    pub fn base_mainnet() -> Self {
        Self {
            name: tycho_models::Chain::Base,
            metadata: alloy_chains::Chain::from_named(NamedChain::Base),
            rpc_url: "https://base-mainnet.infura.io/v3/".to_string(),
            tycho_url: "https://base.tycho.example.com".to_string(),
        }
    }

    #[cfg(test)]
    pub fn unichain_mainnet() -> Self {
        Self {
            name: tycho_models::Chain::Unichain,
            metadata: alloy_chains::Chain::from_named(NamedChain::Unichain),
            rpc_url: "https://unichain-mainnet.infura.io/v3/".to_string(),
            tycho_url: "https://unichain.tycho.example.com".to_string(),
        }
    }
}
