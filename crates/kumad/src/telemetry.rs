use std::{env, sync::OnceLock};

use tracing::{Subscriber, level_filters::LevelFilter};
use tracing_error::ErrorLayer;
use tracing_subscriber::{EnvFilter, Layer, filter::FilterExt, fmt, layer::SubscriberExt as _};

static TELEMETRY_INIT: OnceLock<()> = OnceLock::new();

pub fn get_subscriber() -> impl Subscriber + Send + Sync {
    // use the passed log level or default to RUST_LOG value
    let filter = EnvFilter::from_default_env()
        .add_directive("h2=warn".parse().expect("well-formed"))
        .add_directive(
            "hyper_util=warn"
                .parse()
                .expect("well-formed tracing directive"),
        )
        .add_directive(
            "tycho_client=warn"
                .parse()
                .expect("well-formed tracing directive"),
        )
        .add_directive(
            "tycho_simulation=warn"
                .parse()
                .expect("well-formed tracing directive"),
        )
        .add_directive(
            "alloy_rpc_client=warn"
                .parse()
                .expect("well-formed tracing directive"),
        )
        .add_directive(
            "alloy_pubsub=warn"
                .parse()
                .expect("well-formed tracing directive"),
        )
        .add_directive(
            "alloy_transport_ws=warn"
                .parse()
                .expect("well-formed tracing directive"),
        )
        .add_directive(
            "alloy_json_rpc=warn"
                .parse()
                .expect("well-formed tracing directive"),
        );

    let concise = fmt::layer()
        .with_file(false)
        .with_target(false)
        .with_line_number(false)
        .with_level(true)
        .compact()
        .with_writer(std::io::stdout)
        .with_filter(LevelFilter::INFO);

    let is_verbose = env::var("RUST_VERBOSE")
        .map(|val| val == "1" || val.to_lowercase() == "true")
        .unwrap_or(false);

    let verbose = is_verbose.then_some(
        fmt::layer()
            // .with_file(true)
            // .with_line_number(true)
            .with_level(true)
            .with_writer(std::io::stderr)
            // .compact()
            .with_filter(LevelFilter::TRACE.and(LevelFilter::INFO.not())),
    );

    tracing_subscriber::Registry::default()
        .with(filter)
        .with(ErrorLayer::default())
        .with(verbose)
        .with(concise)
}

pub fn init_subscriber(subscriber: impl Subscriber + Send + Sync) {
    TELEMETRY_INIT
        .set(())
        .expect("global tracing subscriber already set");
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}
