use std::{collections::HashMap, process::ExitCode};

use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::{self, EnvFilter};
use tycho_common::{Bytes, models::Chain};
use tycho_simulation::models::Token;

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

    let tokens: HashMap<Chain, HashMap<Bytes, Token>> = config
        .chains
        .keys()
        .map(|chain| {
            (
                chain.clone(),
                config
                    .tokens
                    .iter()
                    .map(|(symbol, token_config)| {
                        (
                            token_config.addresses[chain].clone(),
                            Token::new(
                                &token_config.addresses[chain].to_string(),
                                token_config.decimals,
                                &symbol,
                                token_config.transfer_gas.into(),
                            ),
                        )
                    })
                    .collect(),
            )
        })
        .collect();

    // set up tycho stream for each chain
    let (chain, chain_info) = config.chains.iter().next().expect("missing chain config");
    let chain_tokens = tokens.get(chain).expect("missing tokens for chain");
    let tycho_stream = tycho::Builder {
        url: "https://api.tycho.xyz".to_string(),
        api_key: "your_api_key".to_string(),
        add_tvl_threshold: 0.0,
        remove_tvl_threshold: 0.0,
        no_tls: false,
        chain_info: chain_info.clone(),
        tokens: chain_tokens.clone(),
    }
    .build();

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

    ExitCode::SUCCESS
}
