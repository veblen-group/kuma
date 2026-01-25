use crate::{
    state::{erc20::Transfer, pair::Pair},
    strategy,
};
use alloy::{primitives::Address, rpc::types::TransactionReceipt};
use color_eyre::eyre::{self};
use num_bigint::BigUint;
use num_traits::CheckedAdd as _;
use serde::{Deserialize, Serialize};
use tycho_simulation::tycho_common::models::token::Token;

/// Realized swap data extracted from an executed transaction receipt.
///
/// Unlike `strategy::Swap` which contains simulated/expected values, this struct
/// contains the actual amounts transferred on-chain, parsed from ERC20 Transfer
/// events in the transaction logs. Used for calculating realized profit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Swap {
    /// The token that was sold in the swap.
    pub token_in: Token,
    /// Actual amount of token_in transferred (from Transfer events).
    pub amount_in: BigUint,
    /// The token that was received in the swap.
    pub token_out: Token,
    /// Actual amount of token_out received (from Transfer events).
    pub amount_out: BigUint,
    /// Gas cost in wei (gas_used * effective_gas_price from receipt).
    pub gas_cost_eth: BigUint,
}

impl Swap {
    /// Parse actual swap amounts from a transaction receipt.
    ///
    /// Iterates through the transaction logs to find ERC20 Transfer events for
    /// the input and output tokens, summing the transferred amounts. Also extracts
    /// the actual gas cost from the receipt.
    pub fn try_from_receipts(
        receipt: &TransactionReceipt,
        swap: strategy::Swap,
    ) -> eyre::Result<Self> {
        let mut amount_in = BigUint::default();
        let mut amount_out = BigUint::default();

        let token_in_addr = Address::from_slice(&swap.token_in.address);
        let token_out_addr = Address::from_slice(&swap.token_out.address);

        for log in receipt.logs() {
            let contract = log.address();

            if contract == token_in_addr {
                let transfer = Transfer::try_from_log(&swap.token_in, log.clone())?;
                amount_in = amount_in
                    .checked_add(&transfer.amount)
                    .ok_or_else(|| eyre::eyre!("overflow adding amount_in"))?;
            } else if contract == token_out_addr {
                let transfer = Transfer::try_from_log(&swap.token_out, log.clone())?;
                amount_out = amount_out
                    .checked_add(&transfer.amount)
                    .ok_or_else(|| eyre::eyre!("overflow adding amount_out"))?;
            }
        }

        let gas_units = BigUint::from(receipt.gas_used);
        let wei_per_gas = BigUint::from(receipt.effective_gas_price);
        // cost in wei, e.g. 5 × 10^14 wei = 500,000 Gwei ~ 0.0005 ETH
        let gas_cost_eth = gas_units * wei_per_gas;

        Ok(Swap {
            token_in: swap.token_in,
            amount_in,
            token_out: swap.token_out,
            amount_out,
            gas_cost_eth,
        })
    }

    pub fn get_pair(&self) -> Pair {
        Pair::new(self.token_in.clone(), self.token_out.clone())
    }
}
