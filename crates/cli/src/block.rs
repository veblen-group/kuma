use core::{collector, config::Config, state::block::Block};
use std::{str::FromStr, sync::Arc};

use color_eyre::eyre::{self, Context as _};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::info;

use tycho_simulation::tycho_common;

#[derive(clap::Args, Debug)]
pub(crate) struct GetBlock {
    /// Chain name to retrieve a block from
    #[arg(long)]
    pub(crate) chain: String,

    /// File path to save the serialized block data to
    #[arg(long = "out-dir", short = 'o')]
    pub(crate) out_dir: String,
}

impl GetBlock {
    pub(crate) async fn run(
        &self,
        cfg: Config,
        shutdown_token: CancellationToken,
    ) -> eyre::Result<()> {
        let (tokens_by_chain, _) = cfg
            .build_addrs_and_inventory()
            .expect("Failed to parse chain assets");

        let Config {
            tycho_api_key,
            add_tvl_threshold,
            remove_tvl_threshold,
            ..
        } = cfg;

        let chain = tokens_by_chain
            .keys()
            .find(|chain| {
                chain.name
                    == tycho_common::models::Chain::from_str(&self.chain)
                        .expect("Invalid chain name provided")
            })
            .expect("Chain not configured");

        // set up tycho stream collectors
        let collector_handle = collector::Builder {
            tycho_url: chain.tycho_url.clone(),
            api_key: tycho_api_key.to_string(),
            add_tvl_threshold,
            remove_tvl_threshold,
            tokens: tokens_by_chain[&chain].clone(),
            chain: chain.clone(),
            shutdown_token,
        }
        .build()
        .wrap_err("failed to start tycho collector for chain : {chain}")?;

        let block = recv_first_block(collector_handle.get_block_rx()).await?;
        info!(%block.height, %chain.name, "reaped block");

        let filename = format!(
            "{}/{}-{}.json",
            self.out_dir,
            chain.name.to_string(),
            block.height
        );

        let json = serde_json::to_string_pretty(&block).wrap_err("failed to serialize block")?;

        Ok(())
    }
}

async fn recv_first_block(mut rx: watch::Receiver<Arc<Option<Block>>>) -> eyre::Result<Block> {
    loop {
        rx.changed()
            .await
            .wrap_err("failed to receive block update")?;

        if let Some(block) = rx.borrow().as_ref().clone() {
            return Ok(block);
        }
    }
}
