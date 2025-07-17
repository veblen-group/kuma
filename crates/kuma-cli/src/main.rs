use std::process::ExitCode;

use clap::{Parser, Subcommand};
use futures::StreamExt as _;
use futures::StreamExt;
use tokio::{
    select,
    signal::unix::{SignalKind, signal},
};
use tracing::{error, info, warn};
use tracing_subscriber::{self, EnvFilter};

use crate::{
    config::Config,
    state::pair::PairStateStream,
    utils::{
        get_chain_pairs, get_chains_from_cli, log_chain_tokens, make_collectors, parse_chain_assets,
    },
};

mod chain;
mod collector;
mod config;
mod signals;
mod state;
mod utils;

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
        add_tvl_threshold,
        remove_tvl_threshold,
        ..
    } = config;

    // Set up signal handlers for graceful shutdown
    let mut sigterm = signal(SignalKind::terminate())
        .expect("setting sigterm listener on unix should always work");
    let mut sigint = signal(SignalKind::interrupt())
        .expect("setting sigint listener on unix should always work");

    let command = tokio::spawn(async move {
        let chain_tokens =
            parse_chain_assets(chains, tokens).expect("Failed to parse chain assets");

        log_chain_tokens(&chain_tokens);

        let cli = Cli::parse();
        let mut pairs = get_chain_pairs(&cli.token_a, &cli.token_b, &chain_tokens);
        let (chain_a, chain_b) = get_chains_from_cli(&cli, &chain_tokens);

        let (collector_a_handle, collector_b_handle) = {
            match make_collectors(
                chain_a.clone(),
                chain_b.clone(),
                &chain_tokens,
                &tycho_api_key,
                add_tvl_threshold,
                remove_tvl_threshold,
            ) {
                Ok(handles) => handles,
                Err(err) => {
                    eprintln!("Failed to start chain collectors: {}", err);
                    return ExitCode::FAILURE;
                }
            }
        };

        if let Commands::GenerateSignals = cli.command {
            info!(command = "generate signals");
            let pair_a = pairs.remove(&chain_a).expect("pair for chain a not found");

            let chain_a_block_rx = collector_a_handle.block_rx();
            let _chain_b_block_rx = collector_b_handle.block_rx();
            let mut chain_a_pair_stream = PairStateStream::from_block_rx(pair_a, chain_a_block_rx);

            // read state from stream
            let block_a = chain_a_pair_stream.next().await.expect("test");

            error!("Block A: {:?}", block_a);

            // precompute data for signal

            // compute arb signal
            // log
        }

        if let Commands::DryRun = cli.command {
            // set up tycho encoder
            // set up signer
        }

        if let Commands::Execute = cli.command {
            // set up submission stuff
        }

        ExitCode::SUCCESS
    });

    // Wait for either command completion or interrupt signal
    let result = select! {
        res = command => {
            res
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
