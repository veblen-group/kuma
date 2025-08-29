use core::config::Config;

use clap::{Parser, Subcommand, command};
use color_eyre::eyre::{self, eyre};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    block,
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

    /// sign permit2 for a token
    #[command(name = "init-permit2")]
    SignPermit2(permit::Permit2),

    GetBlocks(StrategyArgs),
}

impl Cli {
    pub(crate) async fn run(
        self,
        config: Config,
        shutdown_token: CancellationToken,
    ) -> eyre::Result<()> {
        match &self.command {
            Commands::GenerateSignals(args) | Commands::DryRun(args) => {
                let kuma = kuma::Kuma::spawn(config, args.clone(), shutdown_token.clone())
                    .map_err(|e| eyre!("Failed to spawn Kuma: {e:}"))?;

                // Run the command with the Kuma instance
                let signal = kuma.generate_signal().await?;
                info!(%signal, "✅ Generated signal");

                if let Commands::DryRun(_) = self.command {
                    unimplemented!()
                };
            }
            Commands::Execute(_) => {
                unimplemented!()
            }
            Commands::Tokens(cmd) => cmd.run(config).await?,
            Commands::SignPermit2(cmd) => cmd.run(config).await?,
            Commands::GetBlocks(args) => {
                let kuma = kuma::Kuma::spawn(config, args.clone(), shutdown_token.clone())
                    .map_err(|e| eyre!("Failed to spawn Kuma: {e:}"))?;

                let (slow_block, fast_block) = kuma.get_blocks().await?;

                info!(%slow_block.block_height, %fast_block.block_height, "✅ Got blocks");

                let slow_name =
                    format!("block-{}-{}.json", args.slow_chain, slow_block.block_height);

                let fast_name =
                    format!("block-{}-{}.json", args.fast_chain, fast_block.block_height);

                // TODO: slow_block.states implement ProtocolSim + Serialize + Deserialize, use this to serialize them into a file
                // TODO: fast_block.states implement ProtocolSim + Serialize + Deserialize, use this to serialize them into a file
            }
        }
        Ok(())
    }
}
