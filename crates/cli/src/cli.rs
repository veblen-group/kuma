use clap::{Parser, Subcommand};
use color_eyre::eyre::{self, eyre};
use kuma_core::config::Config;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    dryrun::{self},
    execute,
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

    /// Ignore gas costs when determining trade profitability (still track for reporting)
    #[arg(long)]
    pub(crate) ignore_gas_costs_in_profit: bool,

    /// Ignore slippage when determining trade profitability (still track for reporting)
    #[arg(long)]
    pub(crate) ignore_slippage_in_profit: bool,

    /// Ignore congestion fee discount when determining trade profitability (still track for reporting)
    #[arg(long)]
    pub(crate) ignore_congestion_fee_in_profit: bool,

    /// Ignore USDC conversion when determining trade profitability (still track for reporting)
    #[arg(long)]
    pub(crate) ignore_usdc_conversion_in_profit: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Calculate potential arbitrage profit
    #[command(name = "generate-signals")]
    GenerateSignals(StrategyArgs),

    /// Perform a dry run (simulated transaction without execution)
    DryRun(dryrun::DryRun),

    /// Execute arbitrage transaction
    Execute(execute::Execute),

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

                todo!("save signal to file");
            }
            Commands::DryRun(cmd) => {
                cmd.run(config).await?;
            }
            Commands::Execute(cmd) => {
                cmd.run(config).await?;
            }
            Commands::Tokens(cmd) => cmd.run(config).await?,
            Commands::SignPermit2(cmd) => cmd.run(config).await?,
        }
        Ok(())
    }
}
