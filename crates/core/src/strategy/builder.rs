use std::str::FromStr as _;

use color_eyre::eyre::{self, OptionExt};

use crate::{
    config::{Config, InventoriesForChain},
    strategy::CrossChainSingleHop,
};

pub struct Builder {
    pub token_a: String,
    pub token_b: String,
    pub slow_chain_name: String,
    pub fast_chain_name: String,
    pub inventory: InventoriesForChain,
    pub binary_search_steps: usize,
    pub max_slippage_bps: u64,
    pub congestion_risk_discount_bps: u64,
}

impl Builder {
    pub fn build(self) -> eyre::Result<CrossChainSingleHop> {
        let Self {
            token_a,
            token_b,
            slow_chain_name,
            fast_chain_name,
            inventory,
            binary_search_steps,
            max_slippage_bps,
            congestion_risk_discount_bps,
        } = self;

        //  get the pairs for the chains from strategy config
        let chain_pairs = Config::get_chain_pairs(&token_a, &token_b, &inventory);
        //  initialize pair and chain info
        let (slow_chain, fast_chain) = (
            chain_pairs
                .keys()
                .find(|chain| {
                    chain.name
                        == tycho_common::models::Chain::from_str(&slow_chain_name)
                            .expect("invalid slow chain name: {slow_chain_name}")
                })
                .ok_or_eyre("invalid slow chain name")?,
            chain_pairs
                .keys()
                .find(|chain| {
                    chain.name
                        == tycho_common::models::Chain::from_str(&fast_chain_name)
                            .expect("invalid fast chain name: {fast_chain_name}")
                })
                .ok_or_eyre("invalid fast chain name")?,
        );
        let (slow_pair, fast_pair) = (&chain_pairs[&slow_chain], &chain_pairs[&fast_chain]);

        // get inventory
        let slow_inventory = (
            inventory[slow_chain][slow_pair.token_a()].clone(),
            inventory[slow_chain][slow_pair.token_b()].clone(),
        );
        let fast_inventory = (
            inventory[fast_chain][fast_pair.token_a()].clone(),
            inventory[fast_chain][fast_pair.token_b()].clone(),
        );

        Ok(CrossChainSingleHop {
            slow_pair: slow_pair.clone(),
            slow_chain: slow_chain.clone(),
            fast_pair: fast_pair.clone(),
            fast_chain: fast_chain.clone(),
            slow_inventory,
            fast_inventory,
            binary_search_steps,
            max_slippage_bps,
            congestion_risk_discount_bps,
        })
    }
}
