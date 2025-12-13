use std::{
    pin::Pin,
    sync::Arc,
    task::{self, Poll},
};

use alloy::rpc::types::Header;
use futures::{Stream, StreamExt as _};
use num_bigint::BigUint;
use tokio::sync::watch;
use tokio_stream::wrappers::WatchStream;
use tycho_common::models::token::Token;

use crate::state::{
    balances::TokenBalances,
    pair::{Pair, PairState},
    tycho::BlockSim,
};

pub struct Block {
    pub header: Header,
    pub token_balances: TokenBalances,
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

pub struct BlockState {
    pub pair_state: PairState,
    pub token_a_usdc_state: Option<PairState>,
    pub token_a_balance: BigUint,
    pub token_b_usdc_state: Option<PairState>,
    pub token_b_balance: BigUint,
}

#[derive(Debug)]
pub struct BlockStateStream {
    pair: Pair,
    token_a_usdc_pair: Option<Pair>,
    token_b_usdc_pair: Option<Pair>,
    block_rx: WatchStream<Arc<Option<Block>>>,
}

impl BlockStateStream {
    pub fn from_block_rx(
        pair: Pair,
        block_rx: watch::Receiver<Arc<Option<Block>>>,
        usdc: Token,
    ) -> Self {
        // TODO: handle token a/b being usdc by making those pairs optional
        let token_a_usdc_pair = if pair.token_a().symbol != "USDC" {
            Some(Pair::new(pair.token_a().clone(), usdc.clone()))
        } else {
            None
        };
        let token_b_usdc_pair = if pair.token_b().symbol != "USDC" {
            Some(Pair::new(pair.token_b().clone(), usdc))
        } else {
            None
        };

        Self {
            pair,
            token_a_usdc_pair,
            token_b_usdc_pair,
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

                    let token_a_usdc_state =
                        if let Some(token_a_usdc_pair) = &self.token_a_usdc_pair {
                            Some(block.sims.get_pair_state(token_a_usdc_pair))
                        } else {
                            None
                        };
                    let token_b_usdc_state =
                        if let Some(token_b_usdc_pair) = &self.token_b_usdc_pair {
                            Some(block.sims.get_pair_state(token_b_usdc_pair))
                        } else {
                            None
                        };

                    let token_a_balance = block.token_balances.get_balance(self.pair.token_a());
                    let token_b_balance = block.token_balances.get_balance(self.pair.token_b());

                    // TODO: add gas price from header

                    Poll::Ready(Some(BlockState {
                        pair_state,
                        token_a_usdc_state,
                        token_b_usdc_state,
                        token_a_balance,
                        token_b_balance,
                    }))
                }
                // Only start yielding values after the initial block is received
                None => Poll::Pending,
            },
        }
    }
}
