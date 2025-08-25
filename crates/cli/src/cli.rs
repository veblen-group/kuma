use core::{
    config::Config,
    encoder::{get_tx_request, try_transactions_from_signal},
    signals::CrossChainSingleHop,
};
use std::{fs, path::PathBuf, str::FromStr as _};

use alloy::signers::local::PrivateKeySigner;
use clap::{Parser, Subcommand, command};
use color_eyre::eyre::{self, Context, eyre};
use tokio_util::sync::CancellationToken;
use tracing::info;
use tycho_common::models::Address;

use crate::{
    kuma::{self},
    permit, tokens,
};

#[derive(Parser)]
#[command(name = "kuma", about)]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct StrategyArgs {
    /// First token in the pair
    #[arg(long)]
    pub(crate) token_a: String,

    /// Second token in the pair
    #[arg(long)]
    pub(crate) token_b: String,

    /// Slow blockchain for the arbitrage
    #[arg(long)]
    pub(crate) slow_chain: String,

    /// Fast blockchain for the arbitrage
    #[arg(long)]
    pub(crate) fast_chain: String,
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct DryRunArgs {
    /// Input signal path
    #[arg(long)]
    pub(crate) input_path: PathBuf,
    /// Output path for the dry run results
    #[arg(long)]
    pub(crate) output_path: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Calculate potential arbitrage profit
    #[command(name = "generate-signals")]
    GenerateSignals(StrategyArgs),

    /// Perform a dry run (simulated transaction without execution)
    DryRun(DryRunArgs),

    /// Execute arbitrage transaction
    Execute(StrategyArgs),

    /// Get all tokens from tycho api
    Tokens(tokens::Tokens),

    /// sign permit2 for a token
    #[command(name = "init-permit2")]
    SignPermit2(permit::Permit2),
}

impl Cli {
    pub(crate) async fn run(
        self,
        config: Config,
        shutdown_token: CancellationToken,
    ) -> eyre::Result<()> {
        match &self.command {
            Commands::GenerateSignals(args) => {
                let kuma = kuma::Kuma::spawn(config, args.clone(), shutdown_token.clone())
                    .map_err(|e| eyre!("Failed to spawn Kuma: {e:}"))?;

                // Run the command with the Kuma instance
                let signal = kuma.generate_signal().await?;
                info!(%signal, "✅ Generated signal");
            }
            Commands::DryRun(args) => {
                // TODO: read the signal properly

                info!(
                    "Running dry run with signal from: {}",
                    args.input_path.display()
                );

                let signer = PrivateKeySigner::from_str(&config.chains[0].private_key)
                    .wrap_err("Failed to create signer from private key")?;
                let signer_address = signer.address().to_string();
                let signer_tycho_address = Address::from_str(&signer_address)
                    .wrap_err("Failed to parse signer address")?;
                let data = fs::read_to_string(args.input_path.clone())?;
                let signal: CrossChainSingleHop = serde_json::from_str(&data)
                    .wrap_err("Failed to deserialize signal from input file")?;
                let unsigned_transactions = try_transactions_from_signal(
                    signal.clone(),
                    signer_tycho_address,
                    signer.clone(),
                )
                .wrap_err("Failed to create transactions from signal")?;

                // TODO: sign the transaction using alloy with gas estimation
                // TODO: use the `args.output_path` to write the transactions
                let slow_tx = unsigned_transactions.0;
                let fast_tx = unsigned_transactions.1;

                let slow_tx_request = get_tx_request(
                    &slow_tx,
                    &signer,
                    &signal.slow_chain.chain_id(),
                    &signal.slow_chain.rpc_url,
                )
                .await
                .wrap_err("Failed to create transaction request for slow chain")?;
                let fast_tx_request = get_tx_request(
                    &fast_tx,
                    &signer,
                    &signal.fast_chain.chain_id(),
                    &signal.fast_chain.rpc_url,
                )
                .await
                .wrap_err("Failed to create transaction request for fast chain")?;

                info!("Slow transaction: {:#?}", slow_tx_request);
                info!("Fast transaction: {:#?}", fast_tx_request);

                let slow_output_path = args.output_path.with_file_name(format!(
                    "slow_{}",
                    args.output_path.file_name().unwrap().to_str().unwrap()
                ));
                let fast_output_path = args.output_path.with_file_name(format!(
                    "fast_{}",
                    args.output_path.file_name().unwrap().to_str().unwrap()
                ));

                let slow_tx_json = serde_json::to_string_pretty(&slow_tx_request)
                    .wrap_err("Failed to serialize slow transaction")?;
                let fast_tx_json = serde_json::to_string_pretty(&fast_tx_request)
                    .wrap_err("Failed to serialize fast transaction")?;

                fs::write(&slow_output_path, &slow_tx_json).wrap_err_with(|| {
                    format!(
                        "Failed to write slow transaction to file: {}",
                        slow_output_path.display()
                    )
                })?;
                fs::write(&fast_output_path, &fast_tx_json).wrap_err_with(|| {
                    format!(
                        "Failed to write fast transaction to file: {}",
                        fast_output_path.display()
                    )
                })?;

                info!(
                    "Slow transaction written to: {}",
                    slow_output_path.display()
                );
                info!(
                    "Fast transaction written to: {}",
                    fast_output_path.display()
                );
            }
            Commands::Execute(_) => {
                unimplemented!()
            }
            Commands::Tokens(cmd) => cmd.run(config).await?,
            Commands::SignPermit2(cmd) => cmd.run(config).await?,
        }
        Ok(())
    }
}
