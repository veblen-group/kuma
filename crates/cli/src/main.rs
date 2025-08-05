use std::process::ExitCode;

use clap::{Parser, Subcommand, arg};
use color_eyre::eyre::{self, eyre};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use tracing_subscriber::{self, EnvFilter};

// use crate::kuma::Kuma;

use core::config::Config;

mod cli;
mod dry_run;
mod execute;
mod generate_signals;
mod kuma;
mod tokens;

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
    ExitCode::SUCCESS
}
