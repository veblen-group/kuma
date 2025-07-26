use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tokio::{
    select,
    signal::unix::{SignalKind, signal},
};
use tracing::{error, info};
use tracing_subscriber::{self, EnvFilter};

use crate::kuma::Kuma;

use core::{chain, config::Config, state, strategy};

mod kuma;
mod utils;

#[derive(Parser)]
#[command(name = "kuma", about)]
struct Cli {
    /// First token in the pair
    #[arg(long)]
    token_a: String,

    /// Second token in the pair
    #[arg(long)]
    token_b: String,

    /// First blockchain for the arbitrage
    #[arg(long)]
    chain_a: String,

    /// Second blockchain for the arbitrage
    #[arg(long)]
    chain_b: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Calculate potential arbitrage profit
    #[command(name = "generate-signals")]
    GenerateSignals,

    /// Perform a dry run (simulated transaction without execution)
    DryRun,

    /// Execute arbitrage transaction
    Execute,
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
    eprintln!("starting with config:\n{config:?}");

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let kuma = {
        match Kuma::spawn(config, cli) {
            Ok(kuma) => kuma,
            Err(e) => {
                error!(error=%e, "failed to spawn kuma");
                return ExitCode::FAILURE;
            }
        }
    };

    // Set up signal handlers for graceful shutdown
    let mut sigterm = signal(SignalKind::terminate())
        .expect("setting sigterm listener on unix should always work");
    let mut sigint = signal(SignalKind::interrupt())
        .expect("setting sigint listener on unix should always work");

    let command_jh = tokio::spawn(async move { kuma.run().await });

    // Wait for either command completion or interrupt signal
    let result = select! {
        res = command_jh => {
            // TODO: make sure this is correct
            res.and_then(|_| Ok(ExitCode::SUCCESS))
        }
        _ = sigterm.recv() => {
            info!("received SIGTERM signal");
            Ok(ExitCode::FAILURE)
        }
        _ = sigint.recv() => {
            info!("received SIGINT signal");
            Ok(ExitCode::FAILURE)
        }
    };

    match result {
        Ok(_) => {
            info!("command completed successfully");
            ExitCode::SUCCESS
        }
        Err(e) => {
            error!(%e, "command failed");
            ExitCode::FAILURE
        }
    }
}
