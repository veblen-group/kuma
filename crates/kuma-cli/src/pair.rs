use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
    pin::Pin,
    sync::Arc,
    task::{self, Poll},
};

use color_eyre::eyre::Context as _;
use futures::{FutureExt, Stream, StreamExt};
use tokio::{
    pin,
    sync::{broadcast, watch},
};
use tokio_stream::wrappers::{BroadcastStream, WatchStream};
use tycho_simulation::{models::Token, protocol::state::ProtocolSim};

use crate::block::Block;

/// Pair can be more than two tokens
#[derive(Debug, Clone, Eq)]
pub(crate) struct Pair(HashSet<Token>);

impl Pair {
    pub(crate) fn new<I>(tokens: I) -> Self
    where
        I: IntoIterator<Item = Token>,
    {
        Pair(tokens.into_iter().collect())
    }

    pub(crate) fn subset<I>(&self, tokens: &I) -> bool
    where
        I: IntoIterator<Item = Token>,
    {
        self.0.iter().all(|token| tokens.iter().any(|t| t == token))
    }
}

impl PartialEq for Pair {
    fn eq(&self, other: &Self) -> bool {
        self.0.is_subset(&other.0)
    }
}

impl Hash for Pair {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Compute a u64 hash for each token, collect and sort them,
        // then feed them to `state` in sorted order so the final hash
        // is independent of iteration order.
        let mut hashes: Vec<u64> = self
            .0
            .iter()
            .map(|token| {
                let mut h = DefaultHasher::new();
                token.hash(&mut h);
                h.finish()
            })
            .collect();

        hashes.sort_unstable();
        for h in hashes {
            state.write_u64(h);
        }
    }
}

pub(crate) struct PairState {
    pub(crate) block_number: u64,
    pub(crate) unmodified_pools: Arc<HashMap<String, Box<dyn ProtocolSim>>>,
    pub(crate) modified_pools: Arc<HashMap<String, Box<dyn ProtocolSim>>>,
}

#[derive(Debug)]
pub(crate) struct PairStateStream {
    pair: Pair,
    // TODO: pin?
    block_rx: WatchStream<Arc<Block>>,
}

impl PairStateStream {
    pub(crate) fn from_block_rx(pair: Pair, block_rx: watch::Receiver<Arc<Block>>) -> Self {
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
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(block)) => {
                let unmodified_pools = self.unmodified_pools.clone();
                let modified_pools = self.modified_pools.clone();
                Poll::Ready(Some(PairState {
                    block_number: block.block_number,
                    unmodified_pools,
                    modified_pools,
                }))
            }
            Poll::Ready(None) => Poll::Ready(None),
        }
    }
}
