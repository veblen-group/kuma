use std::collections::HashMap;

use alloy::{
    eips::BlockNumberOrTag,
    primitives::{Address, U256},
    providers::Provider,
    rpc::types::Filter,
    sol,
    sol_types::SolEvent as _,
};
use color_eyre::eyre::{self, eyre};
use num_bigint::BigUint;

// Taken from https://github.com/OpenZeppelin/openzeppelin-contracts/blob/3790c59623e99cb0272ddf84e6a17a5979d06b35/contracts/token/ERC20/IERC20.sol
sol!(
    #[sol(rpc)]
    contract IERC20 {
        function balanceOf(address account) external view returns (uint256);
        event Transfer(address indexed from, address indexed to, uint256 value);
    }
);

pub struct TokenBalances {
    account_addr: Address,
    balances: HashMap<Address, BigUint>,
    to_filter: Filter,
    from_filter: Filter,
}

impl TokenBalances {
    pub async fn from_curr_balances<P: Provider + Clone>(
        account_addr: Address,
        token_addrs: Vec<Address>,
        provider: P,
    ) -> eyre::Result<Self> {
        // get token contract handle
        let tokens = token_addrs
            .iter()
            .cloned()
            .map(|addr| IERC20::new(addr, provider.clone()))
            .collect::<Vec<_>>();

        // get initial balances
        let mut balances = HashMap::new();
        for token in &tokens {
            let start: U256 = token.balanceOf(account_addr).call().await?;
            let current_balance = BigUint::from_bytes_be(&start.to_be_bytes::<32usize>());
            balances.insert(token.address().clone(), current_balance);
        }

        let from_filter = Filter::new()
            .address(token_addrs.clone())
            .event(IERC20::Transfer::SIGNATURE)
            .topic1(account_addr)
            .from_block(BlockNumberOrTag::Latest);

        let to_filter = Filter::new()
            .address(token_addrs)
            .event(IERC20::Transfer::SIGNATURE)
            .topic2(account_addr)
            .from_block(BlockNumberOrTag::Latest);

        Ok(Self {
            account_addr,
            balances,
            to_filter,
            from_filter,
        })
    }
}
