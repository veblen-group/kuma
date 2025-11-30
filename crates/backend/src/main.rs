use std::{env, ops::Not as _, sync::OnceLock};

use color_eyre::eyre;
use kuma_backend::spawn;
use kuma_core::config::Config;
use tracing::{level_filters::LevelFilter, Subscriber};
use tracing_error::ErrorLayer;
use tracing_subscriber::{
    filter::{filter_fn, Targets},
    fmt,
    layer::SubscriberExt as _,
    EnvFilter, Layer as _,
};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let subscriber = get_subscriber();
    init_subscriber(subscriber);

    let config = Config::load()?;

    spawn(config).await
}

static TELEMETRY_INIT: OnceLock<()> = OnceLock::new();

pub fn get_subscriber() -> impl Subscriber + Send + Sync {
    // use the passed log level or default to RUST_LOG value
    let directives = Targets::new()
        .with_default(LevelFilter::TRACE)
        .with_target("h2", LevelFilter::WARN)
        .with_target("hyper_util", LevelFilter::WARN)
        .with_target("tycho_client", LevelFilter::WARN)
        .with_target("tycho_simulation", LevelFilter::WARN)
        .with_target("alloy_rpc_client", LevelFilter::WARN)
        .with_target("alloy_pubsub", LevelFilter::WARN)
        .with_target("alloy_transport_ws", LevelFilter::WARN)
        .with_target("alloy_json_rpc", LevelFilter::WARN);

    let is_split_view = env::var("KUMA_SPLIT_VIEW")
        .map(|val| val == "1" || val.to_lowercase() == "true")
        .unwrap_or(false);

    let verbose = is_split_view.then_some(
        fmt::layer()
            // .with_file(true)
            // .with_line_number(true)
            .with_target(false)
            .with_level(true)
            .with_writer(std::io::stderr)
            .compact()
            .with_filter(filter_fn(|metadata| {
                if metadata.is_span() {
                    return true;
                }
                *metadata.level() != tracing::Level::INFO
            })),
    );

    let concise = is_split_view.then_some(
        fmt::layer()
            .with_file(false)
            .with_target(false)
            .with_line_number(false)
            .with_level(true)
            .compact()
            .with_writer(std::io::stdout)
            .with_filter(LevelFilter::INFO),
    );

    let non_split_fmt = is_split_view.not().then_some(
        fmt::layer()
            .with_file(false)
            .with_target(false)
            .with_line_number(false)
            .with_level(true)
            .compact()
            .with_filter(EnvFilter::from_default_env()),
    );

    tracing_subscriber::Registry::default()
        .with(directives)
        .with(ErrorLayer::default())
        .with(verbose)
        .with(concise)
        .with(non_split_fmt)
}

pub fn init_subscriber(subscriber: impl Subscriber + Send + Sync) {
    TELEMETRY_INIT
        .set(())
        .expect("global tracing subscriber already set");
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}
