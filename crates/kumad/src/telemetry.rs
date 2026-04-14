//! Tracing subscriber setup for the kumad binary.
//!
//! Builds a layered `tracing_subscriber` with two modes:
//!
//! **Normal mode** (`KUMA_SPLIT_VIEW` unset or `0`): single compact formatter controlled by
//! `RUST_LOG`. Noisy third-party crates (Tycho internals, Alloy transports, `sqlx`, `h2`)
//! are capped at `WARN` regardless of `RUST_LOG`.
//!
//! **Split-view mode** (`KUMA_SPLIT_VIEW=1`): two formatters running simultaneously —
//! verbose spans/debug to stderr, INFO-only to stdout. Designed for `tmux` split-pane
//! monitoring: pipe stdout to one pane for clean signal/trade events and stderr to another
//! for full trace output.

use std::{env, ops::Not, sync::OnceLock};

use tracing::{Subscriber, level_filters::LevelFilter};
use tracing_error::ErrorLayer;
use tracing_subscriber::{
    EnvFilter, Layer,
    filter::{Targets, filter_fn},
    fmt,
    layer::SubscriberExt as _,
};

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
        .with_target("alloy_json_rpc", LevelFilter::WARN)
        .with_target("sqlx", LevelFilter::WARN);

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
