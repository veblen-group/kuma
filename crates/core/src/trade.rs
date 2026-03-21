use alloy::rpc::types::TransactionReceipt;
use color_eyre::eyre::{self, WrapErr as _};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tracing::{error, instrument};

use crate::{
    encoder::{SignedTransaction, UnsignedTransaction, execute_tx, get_tx_request},
    signals::{self, CrossChainSingleHop, RealizedProfit},
    state,
};

/// Slim write-only structs used only for DB inserts. The full read-back row structs
/// (TradeSuccessRow etc.) include joined signal data and live in database::trade.
pub(crate) struct TradeSuccessInsert {
    pub signal_id: i64,
    pub slow_tx_hash: String,
    pub fast_tx_hash: String,
    pub realized_profit_str: String,
}

pub(crate) struct TradeFailedOnSlowInsert {
    pub signal_id: i64,
    pub slow_tx_hash: Option<String>,
}

pub(crate) struct TradeFailedOnFastInsert {
    pub signal_id: i64,
    pub slow_tx_hash: String,
    pub fast_tx_hash: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TradeFailedOnSlow {
    signal: signals::CrossChainSingleHop,
    signal_id: i64,
    slow_receipt: Option<TransactionReceipt>,
}

impl TradeFailedOnSlow {
    pub(crate) fn into_insert(self) -> TradeFailedOnSlowInsert {
        TradeFailedOnSlowInsert {
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
    pub(crate) fn into_insert(self) -> TradeFailedOnFastInsert {
        TradeFailedOnFastInsert {
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
    pub(crate) fn into_insert(self) -> TradeSuccessInsert {
        TradeSuccessInsert {
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
            self.signal.fast_base_fee,
            &self.signal.fast_swap_sim.gas_cost,
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
            signal: self.signal,
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
