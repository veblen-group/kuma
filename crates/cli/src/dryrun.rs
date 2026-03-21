use kuma_core::{config::Config, encoder::get_tx_request, signals::CrossChainSingleHop};
use std::{fs, path::PathBuf};

use color_eyre::eyre::{self, Context as _};
use tracing::debug;

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct DryRun {
    /// Input signal path
    #[arg(long)]
    pub(crate) input_path: PathBuf,
    /// Output path for the dry run results
    #[arg(long)]
    pub(crate) output_path: PathBuf,
}

impl DryRun {
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
        let slow_tx_request = get_tx_request(trade.slow_tx(), &signal.slow_chain)
            .await
            .wrap_err("Failed to create transaction request for slow chain")?;
        let fast_tx_request = get_tx_request(trade.fast_tx(), &signal.fast_chain)
            .await
            .wrap_err("Failed to create transaction request for fast chain")?;

        let txs = vec![slow_tx_request, fast_tx_request];
        let txs_json =
            serde_json::to_string_pretty(&txs).wrap_err("Failed to serialize transactions")?;
        fs::write(&self.output_path, txs_json).wrap_err_with(|| {
            format!(
                "Failed to write slow transaction to file: {}",
                self.output_path.display()
            )
        })?;

        debug!("Transactions written to: {}", self.output_path.display());
        Ok(())
    }
}
