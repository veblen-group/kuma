use alloy::rpc::types::TransactionReceipt;
use color_eyre::eyre::{self, WrapErr as _, eyre};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tracing::{error, instrument};

use crate::{
    database::{TradeFailedOnFastRow, TradeFailedOnSlowRow, TradeSuccessRow},
    encoder::{SignedTransaction, UnsignedTransaction, execute_tx, get_tx_request},
    signals::{self, CrossChainSingleHop, RealizedProfit},
    state,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TradeFailedOnSlow {
    signal: signals::CrossChainSingleHop,
    signal_id: i64,
    slow_receipt: Option<TransactionReceipt>,
}

impl TradeFailedOnSlow {
    pub fn into_row(self) -> TradeFailedOnSlowRow {
        TradeFailedOnSlowRow {
            signal_id: self.signal_id,
            slow_tx_hash: self
                .slow_receipt
                .map(|receipt| receipt.transaction_hash.to_string()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TradeFailedOnFast {
    signal: signals::CrossChainSingleHop,
    signal_id: i64,
    slow_receipt: TransactionReceipt,
    fast_receipt: Option<TransactionReceipt>,
}

impl TradeFailedOnFast {
    pub fn into_row(self) -> TradeFailedOnFastRow {
        TradeFailedOnFastRow {
            signal_id: self.signal_id,
            slow_tx_hash: self.slow_receipt.transaction_hash.to_string(),
            fast_tx_hash: self
                .fast_receipt
                .map(|receipt| receipt.transaction_hash.to_string()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TradeSuccess {
    signal: signals::CrossChainSingleHop,
    signal_id: i64,
    slow_receipt: TransactionReceipt,
    fast_receipt: TransactionReceipt,
    realized_profit: RealizedProfit,
}

impl TradeSuccess {
    pub fn into_row(self) -> TradeSuccessRow {
        TradeSuccessRow {
            signal_id: self.signal_id,
            slow_tx_hash: self.slow_receipt.transaction_hash.to_string(),
            fast_tx_hash: self.fast_receipt.transaction_hash.to_string(),
            realized_profit_str: self.realized_profit.total_usdc.to_string(),
        }
    }
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
    pub async fn run(self, mut id_rx: oneshot::Receiver<i64>) -> eyre::Result<TradeResult> {
        // TODO: this should be in the expected profit constructor
        let expected_profit_usdc = &self.signal.expected_profit.min_total_amount_usdc;
        let total_gas_cost_usdc = &self.signal.expected_profit.total_gas_cost_usdc;
        if total_gas_cost_usdc > expected_profit_usdc {
            return Err(eyre!(
                "estimated transactions gas cost exceeds expected profit"
            ));
        }
        let slow_receipt = match execute_tx(
            &self.slow_tx_req,
            &self.signal.slow_chain,
            self.signal.expected_profit.slow_base_fee, // TODO: this is wrong
            &self.signal.expected_profit.gas_cost_amounts.0,
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

                let signal_id = id_rx.try_recv().wrap_err("failed to receive signal id")?;

                return Ok(TradeResult::FailedSlow(TradeFailedOnSlow {
                    signal: self.signal.clone(),
                    signal_id,
                    slow_receipt: None,
                }));
            }
        };

        if let Err(error) = Self::check_transaction_success(&slow_receipt) {
            error!(transaction.hash = %slow_receipt.transaction_hash, %error, "slow chain reverted");
            let signal_id = id_rx.try_recv().wrap_err("failed to receive signal id")?;
            return Ok(TradeResult::FailedSlow(TradeFailedOnSlow {
                signal: self.signal.clone(),
                signal_id,
                slow_receipt: Some(slow_receipt),
            }));
        }

        let fast_receipt = match execute_tx(
            self.fast_tx(),
            &self.signal.fast_chain,
            self.signal.expected_profit.fast_base_fee, // TODO: this is wrong
            &self.signal.expected_profit.gas_cost_amounts.1,
        )
        .await
        {
            Ok(receipt) => receipt,
            Err(error) => {
                error!(chain = %self.signal.fast_chain.name, %error, "failed to submit  transaction to fast chain");
                let signal_id = id_rx.try_recv().wrap_err("failed to receive signal id")?;
                return Ok(TradeResult::FailedFast(TradeFailedOnFast {
                    signal: self.signal.clone(),
                    signal_id,
                    slow_receipt,
                    fast_receipt: None,
                }));
            }
        };

        if let Err(error) = Self::check_transaction_success(&fast_receipt) {
            error!(chain = %self.signal.fast_chain.name, %error, "failed to submit  transaction to fast chain");
            let signal_id = id_rx.try_recv().wrap_err("failed to receive signal id")?;
            return Ok(TradeResult::FailedFast(TradeFailedOnFast {
                signal: self.signal.clone(),
                signal_id,
                slow_receipt,
                fast_receipt: Some(fast_receipt),
            }));
        }

        let signal_id = id_rx.try_recv().wrap_err("failed to receive signal id")?;
        let realized_profit = self.calculate_realized_profit(&slow_receipt, &fast_receipt)?;

        Ok(TradeResult::Successful(TradeSuccess {
            signal: self.signal.clone(),
            signal_id,
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
    ) -> eyre::Result<RealizedProfit> {
        let slow_swap =
            state::swap::Swap::try_from_receipts(slow_receipt, self.signal.slow_swap_sim.clone())
                .wrap_err("failed to parse slow swap from receipt")?;
        let fast_swap =
            state::swap::Swap::try_from_receipts(fast_receipt, self.signal.fast_swap_sim.clone())
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
