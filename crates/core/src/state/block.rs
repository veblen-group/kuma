use alloy::rpc::types::Header;

use crate::state::{balances::TokenBalances, tycho::BlockSim};

struct Block {
    header: Header,
    token_balances: TokenBalances,
    sims: BlockSim,
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
