use std::{collections::HashMap, str::FromStr};

use alloy_chains::{self, NamedChain};
use color_eyre::eyre::{self, Context, OptionExt, eyre};
use serde::{Deserialize, Serialize};
use tycho_common::models as tycho_models;
use tycho_simulation::models::Token;

use crate::config::{ChainConfig, TokenConfig};

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
}

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
