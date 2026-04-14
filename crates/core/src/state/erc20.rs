//! ERC20 contract bindings and Transfer event helpers.
//!
//! Provides three things:
//! - [`IERC20Contract`] — a thin handle around the alloy-generated `IERC20` binding,
//!   supporting `balanceOf` for both ERC20 tokens and native ETH (`Address::ZERO`).
//! - [`Transfer`] — decoded ERC20 Transfer event (`from`, `to`, `amount`), used by
//!   `state::swap` to parse realized swap amounts from receipts.
//! - [`latest_from_filter`] / [`latest_to_filter`] — log filter builders for
//!   monitoring Transfer events to/from a given account, used by `TokenBalances`.

use alloy::{
    eips::BlockNumberOrTag,
    primitives::Address,
    providers::Provider,
    rpc::types::{Filter, Log},
    sol,
    sol_types::SolEvent as _,
};
use color_eyre::eyre::{self, Context as _};
use num_bigint::BigUint;
use tycho_common::models::token::Token;

use crate::state::erc20::IERC20::IERC20Instance;

// Taken from https://github.com/OpenZeppelin/openzeppelin-contracts/blob/3790c59623e99cb0272ddf84e6a17a5979d06b35/contracts/token/ERC20/IERC20.sol
sol!(
    #[sol(rpc)]
    contract IERC20 {
        function balanceOf(address account) external view returns (uint256);
        event Transfer(address indexed from, address indexed to, uint256 value);
    }
);

/// Handle for calling IERC20 contract methods
#[derive(Debug, Clone)]
pub struct IERC20Contract<P: Provider + Clone> {
    contract: IERC20Instance<P>,
    pub address: Address,
}

impl<P: Provider + Clone> IERC20Contract<P> {
    pub fn new(address: Address, provider: P) -> Self {
        Self {
            contract: IERC20::new(address, provider),
            address,
        }
    }

    pub async fn balance_of(&self, address: Address) -> eyre::Result<BigUint> {
        if self.address == Address::ZERO {
            // Native ETH balance
            let balance = self.contract.provider().get_balance(address).await?;
            Ok(BigUint::from_bytes_be(&balance.to_be_bytes::<32>()))
        } else {
            // Existing ERC20 logic
            let balance = self.contract.balanceOf(address).call().await?;
            Ok(BigUint::from_bytes_be(&balance.to_be_bytes::<32>()))
        }
    }
}

pub struct Transfer {
    pub from: Address,
    pub to: Address,
    pub token: Token,
    pub amount: BigUint,
}

impl Transfer {
    pub fn try_from_log(token: &Token, log: Log) -> eyre::Result<Self> {
        // decode log
        let event = log
            .log_decode::<IERC20::Transfer>()
            .wrap_err("failed to parse transfer event")?;

        let IERC20::Transfer { from, to, value } = event.inner.data;

        let amount = BigUint::from_bytes_be(&value.to_be_bytes::<32>());

        Ok(Self {
            from,
            to,
            token: token.clone(),
            amount,
        })
    }
}

pub fn latest_from_filter(account_addr: Address, token_addrs: Vec<Address>) -> Filter {
    Filter::new()
        .address(token_addrs)
        .event(IERC20::Transfer::SIGNATURE)
        .topic1(account_addr)
        .from_block(BlockNumberOrTag::Latest)
}

pub fn latest_to_filter(account_addr: Address, token_addrs: Vec<Address>) -> Filter {
    Filter::new()
        .address(token_addrs)
        .event(IERC20::Transfer::SIGNATURE)
        .topic2(account_addr)
        .from_block(BlockNumberOrTag::Latest)
}
