use core::{
    chain::Chain,
    config::Config,
    encoder::{get_tx_request, try_transactions_from_signal},
    signals::CrossChainSingleHop,
};
use std::{fs, path::PathBuf};

use alloy::signers::local::PrivateKeySigner;
use color_eyre::eyre::{self, Context as _, ContextCompat};
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
        let signal: CrossChainSingleHop =
            serde_json::from_str(&data).wrap_err("Failed to deserialize signal from input file")?;

        let (slow_signer, fast_signer) = get_signers(
            &config,
            signal.slow_chain.clone(),
            signal.fast_chain.clone(),
        )?;

        let unsigned_transactions =
            try_transactions_from_signal(signal.clone(), slow_signer.clone(), fast_signer.clone())
                .wrap_err("Failed to create transactions from signal")?;

        let slow_tx = unsigned_transactions.0;
        let fast_tx = unsigned_transactions.1;

        // TODO: get_tx_request should return a signed transaction without broadcasting.
        let slow_tx_request = get_tx_request(
            &slow_tx,
            &slow_signer,
            &signal.slow_chain.chain_id(),
            &signal.slow_chain.rpc_url,
        )
        .await
        .wrap_err("Failed to create transaction request for slow chain")?;
        let fast_tx_request = get_tx_request(
            &fast_tx,
            &fast_signer,
            &signal.fast_chain.chain_id(),
            &signal.fast_chain.rpc_url,
        )
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

fn get_signers(
    config: &Config,
    slow_chain: Chain,
    fast_chain: Chain,
) -> eyre::Result<(PrivateKeySigner, PrivateKeySigner)> {
    let private_key_by_chains = config.build_private_keys_for_chains_id()?;
    let slow_private_key = private_key_by_chains
        .get(&slow_chain.chain_id())
        .wrap_err("Failed to get private key for slow chain")?;
    let fast_private_key = private_key_by_chains
        .get(&fast_chain.chain_id())
        .wrap_err("Failed to get private key for fast chain")?;
    let slow_signer: PrivateKeySigner = slow_private_key
        .parse()
        .wrap_err("Failed to parse private key for slow chain")?;
    let fast_signer: PrivateKeySigner = fast_private_key
        .parse()
        .wrap_err("Failed to parse private key for fast chain")?;
    Ok((slow_signer, fast_signer))
}
