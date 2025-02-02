use std::process::ExitCode;

use kuma::{
    config::{self, Config},
    telemetry::{self, init_subscriber},
};

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
    // get sigterm and await exit reason
    // shutdown

    ExitCode::SUCCESS
}
