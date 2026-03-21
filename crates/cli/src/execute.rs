use kuma_core::{config::Config, signals::CrossChainSingleHop};
use std::{fs, path::PathBuf};

use color_eyre::eyre::{self, Context as _};
use tokio::sync::oneshot;
use tracing::info;

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct Execute {
    /// Input signal path
    #[arg(long)]
    pub(crate) input_path: PathBuf,
}

impl Execute {
    pub(crate) async fn run(&self, config: Config) -> eyre::Result<()> {
        let data = fs::read_to_string(self.input_path.clone())?;
        let mut signal: CrossChainSingleHop =
            serde_json::from_str(&data).wrap_err("Failed to deserialize signal from input file")?;
        let chains = config
            .build_chains()
            .wrap_err("Failed to build chains from config")?;

        let slow_signer = chains
            .iter()
            .find(|c| c.chain_id() == signal.slow_chain.chain_id())
            .ok_or_else(|| eyre::eyre!("Slow chain not found in config"))?;

        let fast_signer = chains
            .iter()
            .find(|c| c.chain_id() == signal.fast_chain.chain_id())
            .ok_or_else(|| eyre::eyre!("Fast chain not found in config"))?;

        // We need to set the private keys for the chains in the signal when running via cli
        signal.slow_chain.private_key = slow_signer.private_key.clone();
        signal.fast_chain.private_key = fast_signer.private_key.clone();

        let trade = signal.try_promote()?;

        let (tx, rx) = oneshot::channel();
        // For CLI execution, we just send a dummy ID as there's no actual database insertion here.
        // In the real kumad daemon, this would be the ID returned from the signal insertion.
        tx.send(0i64)
            .map_err(|_| eyre::eyre!("failed to send dummy signal id"))?;

        let result = trade.run(rx).await?;
        info!("Execution result: {:?}", debug(&result));
        Ok(())
    }
}
