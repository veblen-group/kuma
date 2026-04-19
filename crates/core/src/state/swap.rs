//! Realized swap data parsed from on-chain transaction receipts.
//!
//! [`Swap`] holds the actual amounts transferred in a completed trade, extracted
//! from ERC20 `Transfer` events in the transaction logs. It is distinct from
//! [`strategy::Swap`] which holds simulated/expected values computed before
//! execution. Used by the execution worker to compute `RealizedProfit`.
//!
//! Native ETH inputs/outputs do not emit Transfer events. For those legs the
//! caller must supply an [`EthBalanceDiff`] (pre/post block balance of the signer),
//! fetched asynchronously before calling `try_from_receipts`.

use crate::{
    state::{erc20::Transfer, pair::Pair},
    strategy,
};
use alloy::{primitives::Address, rpc::types::TransactionReceipt};
use color_eyre::eyre::{self};
use num_bigint::BigUint;
use num_traits::{CheckedAdd as _, CheckedSub as _};
use serde::{Deserialize, Serialize};
use tycho_simulation::tycho_common::models::token::Token;

/// Signer ETH balance immediately before and after the swap block.
///
/// Required when either token is native ETH (`Address::ZERO`). The caller
/// fetches these asynchronously via `get_balance` at `block_number - 1` and
/// `block_number` before calling `Swap::try_from_receipts`.
pub struct EthBalanceDiff {
    pub pre: BigUint,
    pub post: BigUint,
}

/// Realized swap data extracted from an executed transaction receipt.
///
/// Unlike `strategy::Swap` which contains simulated/expected values, this struct
/// contains the actual amounts transferred on-chain, parsed from ERC20 Transfer
/// events in the transaction logs. Used for calculating realized profit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Swap {
    /// The token that was sold in the swap.
    pub token_in: Token,
    /// Actual amount of token_in transferred (from Transfer events or balance diff).
    pub amount_in: BigUint,
    /// The token that was received in the swap.
    pub token_out: Token,
    /// Actual amount of token_out received (from Transfer events or balance diff).
    pub amount_out: BigUint,
    /// Gas cost in wei (gas_used * effective_gas_price from receipt).
    pub gas_cost_eth: BigUint,
}

impl Swap {
    /// Parse actual swap amounts from a transaction receipt.
    ///
    /// For ERC20 tokens, iterates through the transaction logs to find Transfer
    /// events, summing the transferred amounts. For native ETH legs, derives the
    /// actual amount from the caller-supplied balance diff:
    /// - ETH in:  `amount_in  = pre − post − gas_cost`
    /// - ETH out: `amount_out = post − pre + gas_cost`
    ///
    /// `eth_balance_diff` must be `Some` when either token is `Address::ZERO`.
    pub fn try_from_receipts(
        receipt: &TransactionReceipt,
        swap: strategy::Swap,
        eth_balance_diff: Option<EthBalanceDiff>,
    ) -> eyre::Result<Self> {
        let mut amount_in = BigUint::default();
        let mut amount_out = BigUint::default();

        let token_in_addr = Address::from_slice(&swap.token_in.address);
        let token_out_addr = Address::from_slice(&swap.token_out.address);

        let gas_units = BigUint::from(receipt.gas_used);
        let wei_per_gas = BigUint::from(receipt.effective_gas_price);
        let gas_cost_eth = gas_units * wei_per_gas;

        if token_in_addr == Address::ZERO || token_out_addr == Address::ZERO {
            let diff = eth_balance_diff
                .ok_or_else(|| eyre::eyre!("eth_balance_diff required for native ETH swap leg"))?;

            if token_in_addr == Address::ZERO {
                // Signer spent amount_in + gas_cost, so balance dropped by that amount.
                amount_in = diff
                    .pre
                    .checked_sub(&diff.post)
                    .ok_or_else(|| eyre::eyre!("ETH balance increased unexpectedly on ETH-in swap"))?
                    .checked_sub(&gas_cost_eth)
                    .ok_or_else(|| eyre::eyre!("balance diff smaller than gas cost on ETH-in swap"))?;
            }

            if token_out_addr == Address::ZERO {
                // Signer received amount_out and paid gas_cost, net change is amount_out - gas_cost.
                amount_out = diff
                    .post
                    .checked_sub(&diff.pre)
                    .ok_or_else(|| eyre::eyre!("ETH balance decreased unexpectedly on ETH-out swap"))?
                    .checked_add(&gas_cost_eth)
                    .ok_or_else(|| eyre::eyre!("overflow computing ETH amount_out"))?;
            }
        }

        for log in receipt.logs() {
            let contract = log.address();

            if contract == token_in_addr && token_in_addr != Address::ZERO {
                let transfer = Transfer::try_from_log(&swap.token_in, log.clone())?;
                amount_in = amount_in
                    .checked_add(&transfer.amount)
                    .ok_or_else(|| eyre::eyre!("overflow adding amount_in"))?;
            } else if contract == token_out_addr && token_out_addr != Address::ZERO {
                let transfer = Transfer::try_from_log(&swap.token_out, log.clone())?;
                amount_out = amount_out
                    .checked_add(&transfer.amount)
                    .ok_or_else(|| eyre::eyre!("overflow adding amount_out"))?;
            }
        }

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
