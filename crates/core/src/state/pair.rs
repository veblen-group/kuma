use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    pin::Pin,
    sync::Arc,
    task::{self, Poll},
};

use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio_stream::wrappers::WatchStream;
use tycho_common::{models::token::Token, simulation::protocol_sim::ProtocolSim};
use tycho_simulation::protocol::models::ProtocolComponent;

use super::block::BlockSim;
use crate::state;

/// Represents a pair of tokens, normalized to Uniswap's zero2one direction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pair(Token, Token);

impl Pair {
    pub fn new(token_a: Token, token_b: Token) -> Self {
        let zero2one = token_a.address < token_b.address;
        if zero2one {
            Self(token_a, token_b)
        } else {
            Self(token_b, token_a)
        }
    }

    pub fn in_token_vec(&self, tokens: &[Token]) -> bool {
        tokens.contains(&self.0) && tokens.contains(&self.1)
    }

    pub fn token_a(&self) -> &Token {
        &self.0
    }

    pub fn token_b(&self) -> &Token {
        &self.1
    }
}

impl Display for Pair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{}", // ({}, {})",
            self.0.symbol,
            self.1.symbol, // self.0.address, self.1.address
        )
    }
}

#[derive(Debug, Clone)]
pub struct PairState {
    pub block_height: u64,
    pub states: HashMap<state::PoolId, Arc<dyn ProtocolSim>>,
    pub modified_pools: Arc<HashSet<state::PoolId>>,

    pub unmodified_pools: Arc<HashSet<state::PoolId>>,

    #[allow(dead_code)]
    pub metadata: HashMap<state::PoolId, Arc<ProtocolComponent>>,
}

#[derive(Debug)]
pub struct PairStateStream {
    pair: Pair,
    block_rx: WatchStream<Arc<Option<BlockSim>>>,
}

impl PairStateStream {
    pub fn from_block_rx(pair: Pair, block_rx: watch::Receiver<Arc<Option<BlockSim>>>) -> Self {
        Self {
            pair,
            block_rx: WatchStream::from_changes(block_rx),
        }
    }
}

impl Stream for PairStateStream {
    type Item = PairState;

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
                    let state = block.get_pair_state(&self.pair);
                    Poll::Ready(Some(state))
                }
                // Only start yielding values after the initial block is received
                None => Poll::Pending,
            },
        }
    }
}
