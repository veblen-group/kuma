use alloy::rpc::types::Header;

use crate::state::{balances::TokenBalances, tycho::BlockSim};

pub struct Block {
    pub header: Header,
    pub token_balances: TokenBalances,
    pub sims: BlockSim,
}

impl Block {
    pub fn from_components(header: Header, token_balances: TokenBalances, sims: BlockSim) -> Self {
        Self {
            header,
            token_balances,
            sims,
        }
    }
}
