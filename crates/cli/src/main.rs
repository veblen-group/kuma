use std::process::ExitCode;

use clap::Parser as _;
use cli::Cli;
use tokio::{
    select,
    signal::unix::{SignalKind, signal},
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use tracing_subscriber::{self, EnvFilter};

// use crate::kuma::Kuma;

use core::config::Config;

mod cli;
mod kuma;
mod permit;
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

    eprintln!("starting with config:\n{config:?}");

    // TODO: move to core
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

    let cli = Cli::parse();
    let shutdown_token = CancellationToken::new();

    let command_jh = tokio::spawn(cli.run(config, shutdown_token));

    // Set up signal handlers for graceful shutdown
    let mut sigterm = signal(SignalKind::terminate())
        .expect("setting sigterm listener on unix should always work");
    let mut sigint = signal(SignalKind::interrupt())
        .expect("setting sigint listener on unix should always work");

    // Wait for either command completion or interrupt signal
    let result = select! {
        res = command_jh => {
            // TODO: make sure this is correct
            res.and_then(|commands_result| {
                match commands_result {
                    Ok(_) => Ok(ExitCode::SUCCESS),
                    Err(e) => {
                        error!(error=%e, "command failed");
                        Ok(ExitCode::FAILURE)
                    },
                }
            })
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
            info!("command completed");
            ExitCode::SUCCESS
        }
        Err(e) => {
            error!(%e, "command exited unexpectedly");
            ExitCode::FAILURE
        }
    }
}
