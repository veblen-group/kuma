//! Module for interacting with Tycho Simulation's ProtocolStream
//! TODO: move this to a simulation submodule and add an execution submodule for the encoder
//! and submission stuff?
use std::pin::Pin;

use color_eyre::eyre;
use futures::Stream;
use http::Uri;
use tokio_util::sync::CancellationToken;
use tracing::error;
use tycho_simulation::{models::Token, protocol::models::BlockUpdate};

use crate::{chain::ChainInfo, tycho};

pub(crate) use tycho::state_update::ChainSpecificAssetState;

mod builder;
pub(crate) mod state_update;

struct Handle {
    chain_info: ChainInfo,
    shutdown_token: CancellationToken,
    join_handle: tokio::task::JoinHandle<eyre::Result<()>>,
    asset_a_state_stream: ChainSpecificAssetState,
    asset_b_state_stream: ChainSpecificAssetState,
}

impl Handle {
    pub(super) fn new(
        chain_info: ChainInfo,
        shutdown_token: CancellationToken,
        join_handle: tokio::task::JoinHandle<eyre::Result<()>>,
        asset_a_state_stream: ChainSpecificAssetState,
        asset_b_state_stream: ChainSpecificAssetState,
    ) -> Self {
        Self {
            chain_info,
            shutdown_token,
            join_handle,
            asset_a_state_stream,
            asset_b_state_stream,
        }
    }

    pub(crate) async fn shutdown(self) -> eyre::Result<()> {
        self.shutdown_token.cancel();
        if let Err(e) = self.join_handle.await {
            error!(chain=?self.chain_info, "Tycho simulation stream worker failed: {}", e);
            return Err(e.into());
        }
        Ok(())
    }

    pub(crate) async fn asset_a_state_stream(&self) -> ChainSpecificAssetState {
        self.asset_a_state_stream.clone()
    }

    pub(crate) async fn asset_b_state_stream(&self) -> ChainSpecificAssetState {
        self.asset_b_state_stream.clone()
    }
}

struct Worker {
    uri: Uri,
    api_key: String,
    protocol_stream: Pin<Box<dyn Stream<Item = BlockUpdate> + Send>>,
    // - api key
    // - token map
    // - protocol stream
    // - channel writers
}

impl Worker {
    pub async fn run(self) -> eyre::Result<()> {
        unimplemented!("connect to stream and feed into asset specific streams");
        // connect to stream
        // let mut protocol_stream = protocol_stream
        //     .auth_key(Some(tycho_api_key.clone()))
        //     .skip_state_decode_failures(true)
        //     .set_tokens(all_tokens.clone())
        //     .await
        //     .build()
        //     .await
        //     .expect("Failed building protocol stream");
        // reap from stream and feed into each channel
    }
}
