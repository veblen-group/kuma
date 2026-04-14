//! Chain configuration and transaction signing.
//!
//! `Chain` is the central configuration object for a single blockchain. It holds:
//! - RPC endpoints (HTTP and WebSocket) for transaction submission and block subscriptions
//! - The Tycho Indexer URL for DEX pool state streaming
//! - The Permit2 contract address (canonical across all EVM chains)
//! - The private key used to sign transactions on this chain
//! - TVL thresholds controlling which AMM pools are tracked
//!
//! ## Slow vs fast chain
//!
//! Each strategy designates one chain as "slow" (longer block time, e.g. Ethereum ~12 s)
//! and one as "fast" (shorter block time, e.g. Base/Unichain ~2 s). The same `Chain`
//! struct is used for both roles — the distinction is purely in how the strategy uses it.
//! See [`crate::strategy`] and `docs/slow-fast-chain-model.md` for details.
//!
//! ## Supported chains
//!
//! Ethereum mainnet, Base, Unichain. Adding a new chain requires entries in the config,
//! a TVL threshold, and registering the appropriate DEX protocols in the Tycho collector.

use std::{
    fmt::{self, Display},
    hash::Hash,
    str::FromStr,
};

use alloy::{primitives::Address, signers::local::PrivateKeySigner};
use alloy_chains::{self, NamedChain};
use color_eyre::eyre::{self, Context, eyre};
use serde::{Deserialize, Serialize};
use tycho_simulation::evm::tycho_models;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chain {
    pub name: tycho_models::Chain,
    pub metadata: alloy_chains::Chain,
    pub rpc_url: String,
    pub rpc_ws_url: String,
    pub tycho_url: String,
    pub permit2_address: Address,
    #[serde(skip)]
    pub private_key: String,
    pub add_tvl_threshold: f64,
    pub remove_tvl_threshold: f64,
}

impl Chain {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: &str,
        rpc_url: &str,
        rpc_ws_url: &str,
        tycho_url: &str,
        permit2_address: &str,
        private_key: &str,
        add_tvl_threshold: f64,
        remove_tvl_threshold: f64,
    ) -> eyre::Result<Self> {
        let name = tycho_models::Chain::from_str(name)
            .wrap_err("failed to parse chain name into tycho::models::Chain")?;
        let metadata = match name {
            tycho_models::Chain::Ethereum => alloy_chains::Chain::from(NamedChain::Mainnet),
            tycho_models::Chain::Base => alloy_chains::Chain::from(NamedChain::Base),
            tycho_models::Chain::Unichain => alloy_chains::Chain::from(NamedChain::Unichain),
            _ => return Err(eyre!("unsupported chain {}", name)),
        };

        let permit2_address =
            Address::from_str(permit2_address).wrap_err("failed to parse address")?;

        // TODO: we cant save it in the struct because it is not serializable - why do we need chain to be serde?
        let _: PrivateKeySigner = private_key
            .parse()
            .wrap_err("failed to parse private key from chain")?;

        Ok(Self {
            name,
            metadata,
            rpc_url: rpc_url.to_string(),
            rpc_ws_url: rpc_ws_url.to_string(),
            tycho_url: tycho_url.to_string(),
            permit2_address,
            private_key: private_key.to_string(),
            add_tvl_threshold,
            remove_tvl_threshold,
        })
    }

    pub fn chain_id(&self) -> u64 {
        self.metadata.id()
    }

    pub fn signer(&self) -> PrivateKeySigner {
        self.private_key
            .parse()
            .expect("private key is already checked in chain constructor")
    }

    #[cfg(test)]
    pub fn eth_mainnet() -> Self {
        Self {
            name: tycho_models::Chain::Ethereum,
            metadata: alloy_chains::Chain::from_named(NamedChain::Mainnet),
            rpc_url: "https://mainnet.infura.io/v3/".to_string(),
            rpc_ws_url: "wss://mainnet.infura.io/ws/v3/".to_string(),
            tycho_url: "tycho-beta.propellerheads.xyz".to_string(),
            permit2_address: Address::from_str("0x000000000022d473030f116ddee9f6b43ac78ba3")
                .expect("Couldn't convert to address"),
            private_key: "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                .to_string(),
            add_tvl_threshold: 5.0,
            remove_tvl_threshold: 1.0,
        }
    }

    #[cfg(test)]
    pub fn base_mainnet() -> Self {
        Self {
            name: tycho_models::Chain::Base,
            metadata: alloy_chains::Chain::from_named(NamedChain::Base),
            rpc_url: "https://base-mainnet.infura.io/v3/".to_string(),
            rpc_ws_url: "wss://base-mainnet.infura.io/ws/v3/".to_string(),
            tycho_url: "tycho-base-beta.propellerheads.xyz".to_string(),
            permit2_address: Address::from_str("0x000000000022d473030f116ddee9f6b43ac78ba3")
                .expect("Couldn't convert to address"),
            private_key: "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                .to_string(),
            add_tvl_threshold: 5.0,
            remove_tvl_threshold: 1.0,
        }
    }

    #[cfg(test)]
    #[allow(unused)]
    pub fn unichain_mainnet() -> Self {
        Self {
            name: tycho_models::Chain::Unichain,
            metadata: alloy_chains::Chain::from_named(NamedChain::Unichain),
            rpc_url: "https://unichain-mainnet.infura.io/v3/".to_string(),
            rpc_ws_url: "wss://unichain-mainnet.infura.io/ws/v3/".to_string(),
            tycho_url: "tycho-unichain-beta.propellerheads.xyz".to_string(),
            permit2_address: Address::from_str("0x000000000022d473030f116ddee9f6b43ac78ba3")
                .expect("Couldn't convert to address"),
            private_key: "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                .to_string(),
            add_tvl_threshold: 0.1,
            remove_tvl_threshold: 0.1,
        }
    }
}

impl Display for Chain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (id={})", self.name, self.chain_id())
    }
}

impl Hash for Chain {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.metadata.hash(state);
        self.rpc_url.hash(state);
        self.rpc_ws_url.hash(state);
        self.tycho_url.hash(state);
        self.permit2_address.hash(state);
        self.private_key.hash(state);
        // ignore f64 fields
    }
}

impl Eq for Chain {}
