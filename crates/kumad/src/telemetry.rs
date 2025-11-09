use std::{env, ops::Not, sync::OnceLock};

use tracing::{Subscriber, level_filters::LevelFilter};
use tracing_error::ErrorLayer;
use tracing_subscriber::{EnvFilter, Layer, filter::filter_fn, fmt, layer::SubscriberExt as _};

static TELEMETRY_INIT: OnceLock<()> = OnceLock::new();

pub fn get_subscriber() -> impl Subscriber + Send + Sync {
    // use the passed log level or default to RUST_LOG value
    let directives = EnvFilter::from_default_env()
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
