use std::sync::OnceLock;

use tracing::{level_filters::LevelFilter, Subscriber};
use tracing_subscriber::{fmt, layer::SubscriberExt as _, EnvFilter};

static TELEMETRY_INIT: OnceLock<()> = OnceLock::new();

pub fn get_subscriber(env_filter: String) -> impl Subscriber + Send + Sync {
    // use the passed log level or default to RUST_LOG value
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .parse(env_filter)
        .expect("failed to parse log level");

    let fmt_layer = fmt::layer().with_file(true).with_line_number(true);

    tracing_subscriber::Registry::default()
        .with(filter)
        .with(fmt_layer)
}

pub fn init_subscriber(subscriber: impl Subscriber + Send + Sync) {
    TELEMETRY_INIT
        .set(())
        .expect("global tracing subscriber already set");
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}
