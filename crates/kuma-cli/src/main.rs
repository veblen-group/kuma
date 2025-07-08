use std::{process::ExitCode, str::FromStr};

use clap::{Parser, Subcommand};
use color_eyre::eyre::{self, Context as _, eyre};
use tokio::{
    select,
    signal::unix::{SignalKind, signal},
};
use tracing::{error, info};
use tracing_subscriber::{self, EnvFilter};

use crate::{chain::parse_chain_assets, config::Config};

mod assets;
mod chain;
mod config;
mod state_update;
mod strategies;
mod tycho;

#[derive(Parser)]
#[command(name = "kuma-cli", about)] // TODO: dont use stupid name
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
    let config = match config::Config::load() {
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

    let Config {
        chains,
        tokens,
        tycho_api_key,
        ..
    } = config;
    let chain_tokens = parse_chain_assets(chains, tokens).expect("Failed to parse chain assets");

    // set up tycho stream for each chain
    let (chain, tokens) = chain_tokens
        .into_iter()
        .find(|(chain, _)| {
            chain.name
                == tycho_common::models::Chain::from_str("ethereum")
                    .expect("Failed to parse eth name")
        })
        .expect("No tokens found for base");
    let tycho_stream = tycho::Builder {
        tycho_url: chain.tycho_url.clone(),
        api_key: tycho_api_key,
        add_tvl_threshold: 100.0,
        remove_tvl_threshold: 100.0,
        tokens: tokens,
        chain,
    }
    .build();

    let mut stream_handle = match tycho_stream {
        Ok(handle) => handle,
        Err(e) => {
            error!(%e, "failed initializing tycho stream");
            return ExitCode::FAILURE;
        }
    };

    let mut sigterm = signal(SignalKind::terminate())
        .expect("setting sigterm listener on unix should always work");

    let cli = Cli::parse();
    match &cli.command {
        Commands::GenerateSignals => {
            info!(command = "generate signals");
        }
        Commands::DryRun => {
            // set up tycho encoder
            // set up signer
        }
        Commands::Execute => {}
    }

    let exit_reason = select! {
        _ = sigterm.recv() => Ok("received SIGTERM"),
        res = &mut stream_handle.worker_handle => {
            match res {
                Ok(inner_res) => {
                    inner_res.expect("worker failed");
                    Ok("exited")
                }
                Err(join_error) => {
                    // Handle the case where the task panicked or was canceled
                    Err(eyre::eyre!("worker task join error: {}", join_error))
                }
            }
        },
    };

    ExitCode::SUCCESS
}
