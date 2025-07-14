use std::{
    collections::{HashMap, HashSet},
    pin::Pin,
    sync::Arc,
    task::{self, Poll},
};

use futures::{Stream, StreamExt};
use tokio::sync::watch;
use tokio_stream::wrappers::WatchStream;
use tycho_simulation::{
    models::Token,
    protocol::{models::ProtocolComponent, state::ProtocolSim},
};

use super::block::Block;
use crate::state;

// TODO: maybe move to assets.rs?
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Pair(Token, Token);

impl Pair {
    pub fn new(token_a: Token, token_b: Token) -> Self {
        Self(token_a, token_b)
    }

    pub fn in_token_vec(&self, tokens: &[Token]) -> bool {
        tokens.contains(&self.0) && tokens.contains(&self.1)
    }
}

pub(crate) struct PairState {
    pub(crate) block_number: u64,
    pub(crate) states: HashMap<state::Id, Arc<dyn ProtocolSim>>,
    pub(crate) modified_pools: Arc<HashSet<state::Id>>,
    pub(crate) unmodified_pools: Arc<HashSet<state::Id>>,
    pub(crate) metadata: HashMap<state::Id, Arc<ProtocolComponent>>,
}

#[derive(Debug)]
pub(crate) struct PairStateStream {
    pair: Pair,
    block_rx: WatchStream<Arc<Option<Block>>>,
}

impl PairStateStream {
    pub(crate) fn from_block_rx(pair: Pair, block_rx: watch::Receiver<Arc<Option<Block>>>) -> Self {
        Self {
            pair,
            block_rx: WatchStream::new(block_rx),
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
