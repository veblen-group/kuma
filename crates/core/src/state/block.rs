use std::{
    pin::Pin,
    sync::Arc,
    task::{self, Poll},
};

use alloy::{consensus::BlockHeader, rpc::types::Header};
use futures::{Stream, StreamExt as _};
use num_bigint::BigUint;
use tokio::sync::watch;
use tokio_stream::wrappers::WatchStream;
use tracing::error;
use tycho_common::models::token::Token;

use crate::state::{
    balances::TokenBalances,
    pair::{Pair, PairState},
    tycho::BlockSim,
};

/// A complete block state containing header, token balances, and pool simulations.
///
/// Represents all the data needed from a single block to perform arbitrage calculations,
/// including the block header (for base fee), account balances, and AMM pool states.
pub struct Block {
    /// The block header containing metadata like block number and base fee.
    pub header: Header,
    /// Token balances for the trading account on this block.
    pub token_balances: TokenBalances,
    /// Pool simulation states for all relevant AMM pools at this block height.
    pub sims: BlockSim,
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

/// Extracted state for a specific trading pair from a block.
///
/// Contains all the information needed by a strategy to generate signals for a
/// particular token pair, including pool states, USDC conversion prices, and gas costs.
pub struct BlockState {
    /// Pool states for the primary trading pair (e.g., WETH/PEPE).
    pub pair_state: PairState,
    /// Pool states for token A to USDC conversion, None if token A is USDC.
    pub token_a_usdc_state: Option<PairState>,
    /// Available balance of token A in the trading account.
    pub token_a_balance: BigUint,
    /// Pool states for token B to USDC conversion, None if token B is USDC.
    pub token_b_usdc_state: Option<PairState>,
    /// Available balance of token B in the trading account.
    pub token_b_balance: BigUint,
    /// Pool states for ETH to USDC conversion, used for gas cost calculation.
    pub eth_usdc_state: Option<PairState>,
    /// Base fee per gas unit in wei, extracted from block header.
    pub base_fee: u64,
}

/// Async stream that yields `BlockState` updates for a specific trading pair.
///
/// Transforms raw block updates into pair-specific state by extracting relevant
/// pool states, balances, and USDC conversion prices. This stream is consumed
/// by strategies to receive real-time block updates for signal generation.
#[derive(Debug)]
pub struct BlockStateStream {
    pair: Pair,
    token_a_usdc_pair: Option<Pair>,
    token_b_usdc_pair: Option<Pair>,
    eth_usdc_pair: Option<Pair>,
    block_rx: WatchStream<Arc<Option<Block>>>,
}

impl BlockStateStream {
    pub fn from_block_rx(
        pair: Pair,
        block_rx: watch::Receiver<Arc<Option<Block>>>,
        usdc: Token,
        eth: Token,
    ) -> Self {
        let token_a_usdc_pair = if pair.token_a().symbol != "USDC" {
            Some(Pair::new(pair.token_a().clone(), usdc.clone()))
        } else {
            None
        };
        let token_b_usdc_pair = if pair.token_b().symbol != "USDC" {
            Some(Pair::new(pair.token_b().clone(), usdc.clone()))
        } else {
            None
        };

        let eth_usdc_pair = Some(Pair::new(eth, usdc));

        Self {
            pair,
            token_a_usdc_pair,
            token_b_usdc_pair,
            eth_usdc_pair,
            block_rx: WatchStream::from_changes(block_rx),
        }
    }
}

impl Stream for BlockStateStream {
    type Item = BlockState;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Option<Self::Item>> {
        // check watch receiver for new block
        let block_poll = self.block_rx.poll_next_unpin(cx);

        match block_poll {
            // Stream itself isn't ready, propagate pending state
            Poll::Pending => Poll::Pending,
            // Stream has ended, end our stream too
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(block)) => match block.as_ref() {
                Some(block) => {
                    let pair_state = block.sims.get_pair_state(&self.pair);

                    let token_a_usdc_state = self
                        .token_a_usdc_pair
                        .as_ref()
                        .map(|token_a_usdc_pair| block.sims.get_pair_state(token_a_usdc_pair));
                    let token_b_usdc_state = self
                        .token_b_usdc_pair
                        .as_ref()
                        .map(|token_b_usdc_pair| block.sims.get_pair_state(token_b_usdc_pair));

                    let eth_usdc_state = self
                        .eth_usdc_pair
                        .as_ref()
                        .map(|eth_usdc_pair| block.sims.get_pair_state(eth_usdc_pair));

                    let token_a_balance = block.token_balances.get_balance(self.pair.token_a());
                    let token_b_balance = block.token_balances.get_balance(self.pair.token_b());

                    let Some(base_fee) = block.header.base_fee_per_gas() else {
                        error!(
                            chain = %self.pair.chain_name(),
                            "block header missing base fee, skipping this block"
                        );
                        return Poll::Pending;
                    };

                    Poll::Ready(Some(BlockState {
                        pair_state,
                        token_a_usdc_state,
                        token_b_usdc_state,
                        token_a_balance,
                        token_b_balance,
                        eth_usdc_state,
                        base_fee,
                    }))
                }
                // Only start yielding values after the initial block is received
                None => Poll::Pending,
            },
        }
    }
}
