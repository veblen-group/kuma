use color_eyre::eyre::{self, OptionExt};

use crate::{
    chain::Chain,
    config::{Config, InventoriesForChain},
    state::pair::Pair,
    strategy::CrossChainSingleHop,
};

pub struct Builder {
    pub token_a: String,
    pub token_b: String,
    pub slow_chain: Chain,
    pub fast_chain: Chain,
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
            slow_chain,
            fast_chain,
            inventory,
            binary_search_steps,
            max_slippage_bps,
            congestion_risk_discount_bps,
        } = self;

        //  get the pairs for the chains from strategy config
        let chain_pairs = Config::get_chain_pairs(&token_a, &token_b, &inventory);
        let (slow_pair, fast_pair) = (&chain_pairs[&slow_chain], &chain_pairs[&fast_chain]);

        // get inventory
        let slow_inventory = (
            inventory[&slow_chain][slow_pair.token_a()].clone(),
            inventory[&slow_chain][slow_pair.token_b()].clone(),
        );
        let fast_inventory = (
            inventory[&fast_chain][fast_pair.token_a()].clone(),
            inventory[&fast_chain][fast_pair.token_b()].clone(),
        );

        let slow_usdc = Config::get_usdc_token(inventory[&slow_chain].keys())
            .ok_or_eyre("No USDC token found for slow chain")?;
        let fast_usdc = Config::get_usdc_token(inventory[&fast_chain].keys())
            .ok_or_eyre("No USDC token found for fast chain")?;

        let slow_token_a_usdc = Pair::new(slow_pair.token_a().clone(), slow_usdc.clone());
        let slow_token_b_usdc = Pair::new(slow_pair.token_b().clone(), slow_usdc.clone());

        let fast_token_a_usdc = Pair::new(fast_pair.token_a().clone(), fast_usdc.clone());
        let fast_token_b_usdc = Pair::new(fast_pair.token_b().clone(), fast_usdc.clone());

        Ok(CrossChainSingleHop {
            slow_pair: slow_pair.clone(),
            slow_usdc,
            slow_chain: slow_chain.clone(),
            fast_pair: fast_pair.clone(),
            fast_usdc,
            fast_chain: fast_chain.clone(),
            slow_inventory,
            fast_inventory,
            binary_search_steps,
            max_slippage_bps,
            congestion_risk_discount_bps,
            slow_token_a_usdc,
            slow_token_b_usdc,
            fast_token_a_usdc,
            fast_token_b_usdc,
        })
    }
}
