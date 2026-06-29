//! Sequential two-leg trade execution.
//!
//! A `Trade` is produced by promoting a `signals::CrossChainSingleHop` via
//! `CrossChainSingleHop::try_promote()`. It holds two pre-encoded `UnsignedTransaction`s
//! (one per chain) and runs them sequentially: slow chain first, fast chain only on success.
//!
//! ## Execution flow (`Trade::run`)
//!
//! 1. Estimate gas for both legs.
//! 2. Submit slow chain transaction and wait for a receipt.
//! 3. If slow chain fails or times out → return `TradeResult::FailedSlow`; fast leg is never submitted.
//! 4. If slow chain succeeds → submit fast chain transaction.
//! 5. If fast chain fails → return `TradeResult::FailedFast` (position must be unwound manually).
//! 6. On full success → parse `Transfer` logs from both receipts to compute `RealizedProfit`.
//!
//! ## Settlement risk rationale
//!
//! Submitting slow-first means the worst-case failure is a failed slow-leg with no open position.
//! A failed fast-leg after a successful slow-leg is the expensive case — sequential ordering
//! minimises this by ensuring the slow-chain price is confirmed before we commit capital on
//! the fast chain.

use alloy::primitives::Address;
use alloy::providers::Provider as _;
use alloy::rpc::types::TransactionReceipt;
use color_eyre::eyre::{self, WrapErr as _};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use tracing::{error, instrument};

use crate::{
    encoder::{SignedTransaction, UnsignedTransaction, execute_tx, get_tx_request},
    signals::{self, CrossChainSingleHop, RealizedProfit},
    state::{self, swap::EthBalanceDiff},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TradeFailedOnSlow {
    pub(crate) signal: signals::CrossChainSingleHop,
    pub(crate) slow_receipt: Option<TransactionReceipt>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TradeFailedOnFast {
    pub(crate) signal: signals::CrossChainSingleHop,
    pub(crate) slow_receipt: TransactionReceipt,
    pub(crate) fast_receipt: Option<TransactionReceipt>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TradeSuccess {
    pub(crate) signal: signals::CrossChainSingleHop,
    pub(crate) slow_receipt: TransactionReceipt,
    pub(crate) fast_receipt: TransactionReceipt,
    pub(crate) realized_profit: RealizedProfit,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum TradeResult {
    Successful(TradeSuccess),
    FailedSlow(TradeFailedOnSlow),
    FailedFast(TradeFailedOnFast),
}

pub struct Trade {
    signal: CrossChainSingleHop,
    slow_tx_req: UnsignedTransaction,
    fast_tx_req: UnsignedTransaction,
}

impl Trade {
    pub(crate) fn new(
        signal: CrossChainSingleHop,
        slow: UnsignedTransaction,
        fast: UnsignedTransaction,
    ) -> Self {
        Trade {
            signal,
            slow_tx_req: slow,
            fast_tx_req: fast,
        }
    }

    pub fn slow_tx(&self) -> &UnsignedTransaction {
        &self.slow_tx_req
    }

    pub fn fast_tx(&self) -> &UnsignedTransaction {
        &self.fast_tx_req
    }

    // Prepare the trade by creating the transaction requests for both chains
    pub async fn prepare(&self) -> eyre::Result<(SignedTransaction, SignedTransaction)> {
        let slow_tx_request = get_tx_request(self.slow_tx(), &self.signal.slow_chain)
            .await
            .wrap_err("Failed to create transaction request for slow chain")?;
        let fast_tx_request = get_tx_request(self.fast_tx(), &self.signal.fast_chain)
            .await
            .wrap_err("Failed to create transaction request for fast chain")?;
        Ok((slow_tx_request, fast_tx_request))
    }

    // Execute the trade by sending the transactions to their respective chains
    #[instrument(skip(self), fields(slow_chain = %self.signal.slow_chain.name, fast_chain = %self.signal.fast_chain.name))]
    pub async fn run(self) -> eyre::Result<TradeResult> {
        let slow_receipt = match execute_tx(
            &self.slow_tx_req,
            &self.signal.slow_chain,
            self.signal.slow_base_fee,
            &self.signal.slow_swap_sim.gas_cost,
        )
        .await
        {
            Ok(receipt) => receipt,
            Err(error) => {
                error!(
                    chain = %self.signal.slow_chain.name,
                    %error,
                    "failed to submit transaction to slow chain",
                );

                return Ok(TradeResult::FailedSlow(TradeFailedOnSlow {
                    signal: self.signal.clone(),
                    slow_receipt: None,
                }));
            }
        };

        if let Err(error) = Self::check_transaction_success(&slow_receipt) {
            error!(transaction.hash = %slow_receipt.transaction_hash, %error, "slow chain reverted");
            return Ok(TradeResult::FailedSlow(TradeFailedOnSlow {
                signal: self.signal.clone(),
                slow_receipt: Some(slow_receipt),
            }));
        }

        let fast_receipt = match execute_tx(
            self.fast_tx(),
            &self.signal.fast_chain,
            self.signal.fast_base_fee,
            &self.signal.fast_swap_sim.gas_cost,
        )
        .await
        {
            Ok(receipt) => receipt,
            Err(error) => {
                error!(chain = %self.signal.fast_chain.name, %error, "failed to submit  transaction to fast chain");
                return Ok(TradeResult::FailedFast(TradeFailedOnFast {
                    signal: self.signal.clone(),
                    slow_receipt,
                    fast_receipt: None,
                }));
            }
        };

        if let Err(error) = Self::check_transaction_success(&fast_receipt) {
            error!(chain = %self.signal.fast_chain.name, %error, "failed to submit  transaction to fast chain");
            return Ok(TradeResult::FailedFast(TradeFailedOnFast {
                signal: self.signal.clone(),
                slow_receipt,
                fast_receipt: Some(fast_receipt),
            }));
        }

        let slow_eth_diff = fetch_eth_balance_diff_if_needed(
            &slow_receipt,
            &self.signal.slow_swap_sim,
            &self.signal.slow_chain.rpc_url,
        )
        .await
        .wrap_err("failed to fetch ETH balance diff for slow swap")?;

        let fast_eth_diff = fetch_eth_balance_diff_if_needed(
            &fast_receipt,
            &self.signal.fast_swap_sim,
            &self.signal.fast_chain.rpc_url,
        )
        .await
        .wrap_err("failed to fetch ETH balance diff for fast swap")?;

        let realized_profit =
            self.calculate_realized_profit(&slow_receipt, &fast_receipt, slow_eth_diff, fast_eth_diff)?;

        Ok(TradeResult::Successful(TradeSuccess {
            signal: self.signal,
            slow_receipt,
            fast_receipt,
            realized_profit,
        }))
    }

    fn check_transaction_success(receipt: &TransactionReceipt) -> eyre::Result<()> {
        if receipt.status() {
            Ok(())
        } else {
            Err(eyre::eyre!("transaction reverted"))
        }
    }

    fn calculate_realized_profit(
        &self,
        slow_receipt: &TransactionReceipt,
        fast_receipt: &TransactionReceipt,
        slow_eth_diff: Option<EthBalanceDiff>,
        fast_eth_diff: Option<EthBalanceDiff>,
    ) -> eyre::Result<RealizedProfit> {
        let slow_swap =
            state::swap::Swap::try_from_receipts(slow_receipt, self.signal.slow_swap_sim.clone(), slow_eth_diff)
                .wrap_err("failed to parse slow swap from receipt")?;
        let fast_swap =
            state::swap::Swap::try_from_receipts(fast_receipt, self.signal.fast_swap_sim.clone(), fast_eth_diff)
                .wrap_err("failed to parse fast swap from receipt")?;

        let profit = RealizedProfit::try_from_swaps(
            &slow_swap,
            &fast_swap,
            self.signal.slow_prices_a_usdc.clone(),
            self.signal.slow_prices_b_usdc.clone(),
            None, // TODO: provide eth_usdc prices
        )
        .wrap_err("failed to calculate realized profit")?;

        Ok(profit)
    }
}

/// Fetches the signer's ETH balance at block N-1 and block N when the swap has a
/// native ETH leg. Returns `None` for ERC20-only swaps (no RPC call needed).
async fn fetch_eth_balance_diff_if_needed(
    receipt: &TransactionReceipt,
    swap_sim: &crate::strategy::Swap,
    rpc_url: &str,
) -> eyre::Result<Option<EthBalanceDiff>> {
    let token_in_addr = Address::from_slice(&swap_sim.token_in.address);
    let token_out_addr = Address::from_slice(&swap_sim.token_out.address);

    if token_in_addr != Address::ZERO && token_out_addr != Address::ZERO {
        return Ok(None);
    }

    let provider = alloy::providers::ProviderBuilder::new()
        .connect_http(rpc_url.parse().wrap_err("invalid RPC URL")?);

    let from = receipt.from;
    let block_number = receipt
        .block_number
        .ok_or_else(|| eyre::eyre!("receipt missing block number"))?;
    let pre_block = block_number
        .checked_sub(1)
        .ok_or_else(|| eyre::eyre!("block number is 0, cannot query pre-block balance"))?;

    let pre_balance = provider
        .get_balance(from)
        .block_id(pre_block.into())
        .await
        .map_err(|e| eyre::eyre!("failed to get pre-block ETH balance: {e}"))?;
    let post_balance = provider
        .get_balance(from)
        .block_id(block_number.into())
        .await
        .map_err(|e| eyre::eyre!("failed to get post-block ETH balance: {e}"))?;

    Ok(Some(EthBalanceDiff {
        pre: BigUint::from_bytes_be(&pre_balance.to_be_bytes::<32>()),
        post: BigUint::from_bytes_be(&post_balance.to_be_bytes::<32>()),
    }))
}
