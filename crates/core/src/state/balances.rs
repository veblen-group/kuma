use std::{
    collections::HashMap,
    fmt::{self, Display},
};

use alloy::{primitives::Address, providers::Provider, rpc::types::Filter};
use color_eyre::eyre::{self, Context as _, OptionExt as _};
use num_bigint::BigUint;
use tracing::trace;
use tycho_simulation::tycho_core::models::token::Token;

use crate::state::erc20::{IERC20Contract, Transfer, latest_from_filter, latest_to_filter};

#[derive(Debug, Clone)]
pub struct TokenBalances {
    account_addr: Address,
    // TODO: keep the address -> symbol for display purposes, but prob under an arc for cheaper clones
    tokens: HashMap<Address, Token>,
    balances: HashMap<Token, BigUint>,
    to_filter: Filter,
    from_filter: Filter,
}

impl TokenBalances {
    pub fn get_balance(&self, token: &Token) -> BigUint {
        self.balances.get(token).cloned().unwrap_or_default()
    }

    pub async fn get_curr_balances(
        account_addr: Address,
        token_addrs: HashMap<Address, Token>,
        provider: impl Provider + Clone,
    ) -> eyre::Result<Self> {
        // get token contract handle
        let token_contracts = token_addrs
            .keys()
            .cloned()
            .map(|addr| IERC20Contract::new(addr, provider.clone()))
            .collect::<Vec<_>>();

        // get initial balances
        let mut balances = HashMap::new();
        for contract in &token_contracts {
            let current_balance = contract.balance_of(account_addr).await?;
            let token = token_addrs
                .get(&contract.address)
                .ok_or_eyre("missing token metadata for contract")?;
            balances.insert(token.clone(), current_balance);
        }

        let from_filter = latest_from_filter(account_addr, token_addrs.keys().cloned().collect());

        let to_filter = latest_to_filter(account_addr, token_addrs.keys().cloned().collect());

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
            let contract_addr = log.address();
            let Some(token) = self.tokens.get(&contract_addr) else {
                trace!(addrss = %contract_addr, "transfer event for unknown token, skipping");
                continue;
            };
            let transfer =
                Transfer::try_from_log(token, log).wrap_err("failed to parse transfer log")?;

            // update balance
            self.balances
                .entry(token.clone())
                .and_modify(|curr| *curr += transfer.amount);
        }

        let from_logs = provider
            .get_logs(&self.from_filter)
            .await
            .wrap_err("failed to get transfer logs from account addr")?;

        for log in from_logs {
            let contract_addr = log.address();
            let Some(token) = self.tokens.get(&contract_addr) else {
                trace!(addrss = %contract_addr, "transfer event for unknown token, skipping");
                continue;
            };
            let transfer =
                Transfer::try_from_log(token, log).wrap_err("failed to parse transfer log")?;

            // update balance
            self.balances
                .entry(token.clone())
                .and_modify(|curr| *curr -= transfer.amount);
        }

        Ok(())
    }
}

impl Display for TokenBalances {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let balances_str = self
            .balances
            .iter()
            .map(|(token, bal)| format!("{}: {}", token.symbol, bal))
            .collect::<Vec<_>>()
            .join(", ");

        // Keep it on one line for cleaner log parsing
        write!(f, "[{}] Balances: {}", self.account_addr, balances_str)
    }
}
