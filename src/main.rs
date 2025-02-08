use std::process::ExitCode;

use color_eyre::eyre::{self, eyre};
use kuma::{
    config::{self, Config},
    telemetry::{self, init_subscriber},
    Kuma,
};
use tokio::{
    select,
    signal::unix::{signal, SignalKind},
};
use tracing::{error, info, instrument, warn};

#[tokio::main]
async fn main() -> ExitCode {
    println!("Hello, world!");

    // set up config
    let cfg: Config = match config::get() {
        Err(err) => {
            eprintln!("failed to read config:\n{err:?}");
            return ExitCode::FAILURE;
        }
        Ok(cfg) => cfg,
    };
    eprintln!("starting with config:\n{cfg:?}");

    // set up tracing
    let tracing_subscriber = telemetry::get_subscriber(cfg.log_level.to_string());
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
