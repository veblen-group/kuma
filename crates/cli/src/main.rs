use std::process::ExitCode;

use clap::{Parser, Subcommand, arg};
use color_eyre::eyre;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use tracing_subscriber::{self, EnvFilter};

// use crate::kuma::Kuma;

use core::config::Config;

mod dry_run;
mod execute;
mod generate_signals;
mod kuma;
mod tokens;

#[derive(Parser)]
#[command(name = "kuma", about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Args, Debug, Clone)]
struct StrategyArgs {
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
    GenerateSignals(generate_signals::GenerateSignals),

    /// Perform a dry run (simulated transaction without execution)
    DryRun(dry_run::DryRun),

    /// Execute arbitrage transaction
    Execute(execute::Execute),

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
            Commands::GenerateSignals(cmd) => cmd.run(config, shutdown_token).await,
            Commands::DryRun(_) => unimplemented!("DryRun command is not implemented yet"),
            Commands::Execute(_) => unimplemented!("Execute command is not implemented yet"),
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

#[tokio::main]
async fn main() -> ExitCode {
    // Load configuration
    let config = match Config::load() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("Failed to load configuration: {}", err);
            return ExitCode::FAILURE;
        }
    };

    // eprintln!("starting with config:\n{config:?}");

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(
                    "tycho_client=warn"
                        .parse()
                        .expect("well-formed tracing directive should parse"),
                )
                .add_directive(
                    "tycho_simulation=warn"
                        .parse()
                        .expect("well-formed tracing directive should parse"),
                ),
        )
        .with_target(false)
        .init();

    let shutdown_token = CancellationToken::new();

    let cli = Cli::parse();
    cli.run(config, shutdown_token).await.unwrap_or_else(|e| {
        error!(error = %e, "Command execution failed");
        ()
    });
    // let kuma = {
    //     match Kuma::spawn(config, cli, shutdown_token) {
    //         Ok(kuma) => kuma,
    //         Err(e) => {
    //             error!(error=%e, "failed to spawn kuma");
    //             return ExitCode::FAILURE;
    //         }
    //     }
    // };

    // // Set up signal handlers for graceful shutdown
    // let mut sigterm = signal(SignalKind::terminate())
    //     .expect("setting sigterm listener on unix should always work");
    // let mut sigint = signal(SignalKind::interrupt())
    //     .expect("setting sigint listener on unix should always work");

    // let command_jh = tokio::spawn(async move { kuma.run().await });

    // // Wait for either command completion or interrupt signal
    // let result = select! {
    //     res = command_jh => {
    //         // TODO: make sure this is correct
    //         res.and_then(|commands_result| {
    //             match commands_result {
    //                 Ok(_) => Ok(ExitCode::SUCCESS),
    //                 Err(e) => {
    //                     error!(error=%e, "command failed");
    //                     Ok(ExitCode::FAILURE)
    //                 },
    //             }
    //         })
    //     }
    //     _ = sigterm.recv() => {
    //         info!("received SIGTERM signal");
    //         Ok(ExitCode::FAILURE)
    //     }
    //     _ = sigint.recv() => {
    //         info!("received SIGINT signal");
    //         Ok(ExitCode::FAILURE)
    //     }
    // };

    // match result {
    //     Ok(_) => {
    //         info!("command completed successfully");
    //         ExitCode::SUCCESS
    //     }
    //     Err(e) => {
    //         error!(%e, "command failed");
    //         ExitCode::FAILURE
    //     }
    // }
    ExitCode::SUCCESS
}
