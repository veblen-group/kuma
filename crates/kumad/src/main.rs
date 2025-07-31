use std::process::ExitCode;

use color_eyre::eyre::{self, eyre};
use kuma_core::config::Config;
use kumad::{
    Kuma,
    telemetry::{self, init_subscriber},
};
use tokio::{
    select,
    signal::unix::{SignalKind, signal},
};
use tracing::{error, info, instrument, warn};

#[tokio::main]
async fn main() -> ExitCode {
    println!("Hello, world!");

    // set up config
    let cfg: Config = match Config::load() {
        Err(err) => {
            eprintln!("failed to read config:\n{err:?}");
            return ExitCode::FAILURE;
        }
        Ok(cfg) => cfg,
    };
    eprintln!("starting with config:\n{cfg:?}");

    // set up tracing
    let tracing_subscriber = telemetry::get_subscriber();
    init_subscriber(tracing_subscriber);

    // spawn service
    let mut kuma = match Kuma::spawn(cfg) {
        Ok(kuma) => kuma,
        Err(e) => {
            error!(%e, "failed initializing kuma");
            return ExitCode::FAILURE;
        }
    };

    let mut sigterm = signal(SignalKind::terminate())
        .expect("setting sigterm listener on unix should always work");

    let exit_reason = select! {
        _ = sigterm.recv() => Ok("received SIGTERM"),
        res = &mut kuma => {
            res.and_then(|()| Err(eyre!("kuma service exited")))
        },
    };

    shutdown(exit_reason, kuma).await
}

#[instrument(skip_all)]
async fn shutdown(reason: eyre::Result<&str>, service: Kuma) -> ExitCode {
    // TODO: add &str reason
    let exit_code = match reason {
        Ok(reason) => {
            info!(reason, "shutting down");
            if let Err(e) = service.shutdown().await {
                warn!(%e, "shutting down");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            error!(%e, "kuma service exited unexpectedly");
            ExitCode::FAILURE
        }
    };
    info!("shutdown successful");
    exit_code
}
