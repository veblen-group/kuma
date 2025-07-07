use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::{Stream, StreamExt as _};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tycho_simulation::models::Token;

#[derive(Debug)]
pub(crate) struct ChainSpecificAssetState {
    asset_a: Token,
    asset_b: Token,
    tx: broadcast::Sender<AssetStateUpdate>,
    rx: BroadcastStream<AssetStateUpdate>,
    // TODO: store all_pools or something
}

impl Clone for ChainSpecificAssetState {
    fn clone(&self) -> Self {
        let rx = self.tx.subscribe();
        Self {
            asset_a: self.asset_a.clone(),
            asset_b: self.asset_b.clone(),
            tx: self.tx.clone(),
            rx: BroadcastStream::new(rx),
        }
    }
}

impl Stream for ChainSpecificAssetState {
    type Item = AssetStateUpdate;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // TODO: what else?
        let _block_update = self.rx.poll_next_unpin(cx);

        unimplemented!("transform block_update into asset_state_update")
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AssetStateUpdate {}
