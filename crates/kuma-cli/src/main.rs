use std::{process::ExitCode, str::FromStr};

use clap::{Parser, Subcommand};
use color_eyre::eyre::{self, eyre};
use tokio::{
    select,
    signal::unix::{SignalKind, signal},
};
use tracing::{error, info, warn};
use tracing_subscriber::{self, EnvFilter};

use crate::{chain::parse_chain_assets, config::Config};

mod block;
mod chain;
mod collector;
mod config;
mod pair;
mod strategies;

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
    let tycho_stream = collector::Builder {
        tycho_url: chain.tycho_url.clone(),
        api_key: tycho_api_key,
        add_tvl_threshold: 100.0,
        remove_tvl_threshold: 100.0,
        tokens: tokens,
        chain,
    }
    .build();

    // set up stream
    let mut stream_handle = match tycho_stream {
        Ok(handle) => handle,
        Err(e) => {
            error!(%e, "failed initializing tycho stream");
            return ExitCode::FAILURE;
        }
    };

    let cli = Cli::parse();
    if let Commands::GenerateSignals = cli.command {
        info!(command = "generate signals");
        // TODO: read a block from stream
    }

    if let Commands::DryRun = cli.command {
        // set up tycho encoder
        // set up signer
    }

    if let Commands::Execute = cli.command {
        // set up submission stuff
    }

    let mut sigterm = signal(SignalKind::terminate())
        .expect("setting sigterm listener on unix should always work");
    let exit_reason = select! {
        _ = sigterm.recv() => {
            let stream_res = stream_handle.shutdown().await;
            info!(stream_res = ?stream_res);
            Ok("received SIGTERM")
        },
        res = &mut stream_handle => {
            // will only return without shutdown if the collector's worker dies unexpectedly
            res.and_then(|()| Err(eyre!("collector stream exited unexpectedly")))
        },
    };

    shutdown(exit_reason, stream_handle).await
}

async fn shutdown(
    reason: eyre::Result<&'static str>,
    mut collector: collector::Handle,
) -> ExitCode {
    let exit_reason = match reason {
        Ok(reason) => {
            info!(reason, "shutting down");
            if let Err(e) = collector.shutdown().await {
                warn!(%e, "error occurred while shutting down collector");
            };
            ExitCode::SUCCESS
        }
        Err(reason) => {
            error!(%reason, "shutting down");
            ExitCode::FAILURE
        }
    };
    info!("shutdown successful");

    exit_reason
}
