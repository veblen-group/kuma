use std::{
    collections::HashMap,
    fmt::{self, Display},
};

use alloy::{
    eips::BlockNumberOrTag,
    primitives::{Address, U256},
    providers::Provider,
    rpc::types::Filter,
    sol,
    sol_types::SolEvent as _,
};
use color_eyre::eyre::{self, Context as _};
use num_bigint::BigUint;
use tycho_simulation::tycho_core::models::token::Token;

// Taken from https://github.com/OpenZeppelin/openzeppelin-contracts/blob/3790c59623e99cb0272ddf84e6a17a5979d06b35/contracts/token/ERC20/IERC20.sol
sol!(
    #[sol(rpc)]
    contract IERC20 {
        function balanceOf(address account) external view returns (uint256);
        event Transfer(address indexed from, address indexed to, uint256 value);
    }
);

#[derive(Debug, Clone)]
pub struct TokenBalances {
    account_addr: Address,
    // TODO: keep the address -> symbol for display purposes, but prob under an arc for cheaper clones
    tokens: HashMap<Address, Token>,
    balances: HashMap<Address, BigUint>,
    to_filter: Filter,
    from_filter: Filter,
}

impl TokenBalances {
    pub fn get_balance(&self, token: &Token) -> BigUint {
        self.balances
            .get(&Address::from_slice(&token.address))
            .cloned()
            .unwrap_or_default()
    }

    pub async fn get_curr_balances(
        account_addr: Address,
        token_addrs: Vec<Address>,
        provider: impl Provider + Clone,
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
            balances.insert(*token.address(), current_balance);
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
            tokens: HashMap::new(), // TODO: fix this
            balances,
            to_filter,
            from_filter,
        })
    }

    /// Fetch logs for transfers to/from account address and update balances in place
    pub async fn update_from_logs(&mut self, provider: impl Provider + Clone) -> eyre::Result<()> {
        let to_logs = provider
            .get_logs(&self.to_filter)
            .await
            .wrap_err("failed to get transfer logs to account addr")?;

        for log in to_logs {
            // decode log
            let event = log
                .log_decode::<IERC20::Transfer>()
                .wrap_err("failed to parse transfer event")?;
            let IERC20::Transfer { from: _, to, value } = event.inner.data;
            let value = BigUint::from_bytes_be(&value.to_be_bytes::<32>());

            // update balance
            self.balances.entry(to).and_modify(|curr| *curr += value);
        }

        let from_logs = provider
            .get_logs(&self.from_filter)
            .await
            .wrap_err("failed to get transfer logs from account addr")?;

        for log in from_logs {
            // decode log
            let event = log
                .log_decode::<IERC20::Transfer>()
                .wrap_err("failed to parse transfer event")?;
            let IERC20::Transfer { from, to: _, value } = event.inner.data;
            let value = BigUint::from_bytes_be(&value.to_be_bytes::<32>());

            // update balance
            self.balances.entry(from).and_modify(|curr| *curr -= value);
        }

        Ok(())
    }
}

impl Display for TokenBalances {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let balances = self
            .balances
            .iter()
            .map(|(addr, bal)| format!("{}: {}", self.tokens[addr].symbol, bal))
            .collect::<Vec<_>>()
            .join(", ");
        write!(
            f,
            "TokenBalances {{\n
                account_addr: {}\n,
                balances: \n{:?},\n
            }}",
            self.account_addr, balances
        )
    }
}
