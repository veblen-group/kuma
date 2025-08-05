use core::config::Config;

use clap::{Parser, Subcommand, command};
use color_eyre::eyre;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{kuma, tokens};

#[derive(Parser)]
#[command(name = "kuma", about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct StrategyArgs {
    /// First token in the pair
    #[arg(long)]
    token_a: String,

    /// Second token in the pair
    #[arg(long)]
    token_b: String,

    /// Slow blockchain for the arbitrage
    #[arg(long)]
    slow_chain: String,

    /// Fast blockchain for the arbitrage
    #[arg(long)]
    fast_chain: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Calculate potential arbitrage profit
    #[command(name = "generate-signals")]
    GenerateSignals(StrategyArgs),

    /// Perform a dry run (simulated transaction without execution)
    DryRun(StrategyArgs),

    /// Execute arbitrage transaction
    Execute(StrategyArgs),

    /// Get all tokens from tycho api
    Tokens(tokens::Tokens),
}

impl Cli {
    async fn run(self, config: Config, shutdown_token: CancellationToken) -> eyre::Result<()> {
        let (tokens_by_chain, _inventory) = config
            .build_addrs_and_inventory()
            .expect("Failed to parse chain assets");

        info!("Parsed {} chains from config:", tokens_by_chain.len());

        for (chain, _tokens) in &tokens_by_chain {
            info!(chain.name = %chain.name,
                        chain.id = %chain.metadata.id(),
                        "🔗 Initialized chain info from config");
        }

        match self.command {
            Commands::GenerateSignals(args) | Commands::DryRun(args) | Commands::Execute(args) => {
                // Ensure the Kuma instance is created only once
                let kuma = kuma::Kuma::spawn(config, args, shutdown_token.clone())
                    .map_err(|e| eyre!("Failed to spawn Kuma: {e:}"))?;

                // Run the command with the Kuma instance
                kuma.run(cmd).await
            }
            Commands::Tokens(cmd) => {
                cmd.run(
                    tokens_by_chain.keys().cloned().collect(),
                    &config.tycho_api_key,
                )
                .await
            }
        }
    }
}
