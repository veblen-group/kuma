use std::sync::OnceLock;

use tracing::Subscriber;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt as _};

static TELEMETRY_INIT: OnceLock<()> = OnceLock::new();

pub fn get_subscriber() -> impl Subscriber + Send + Sync {
    // use the passed log level or default to RUST_LOG value
    let filter = EnvFilter::from_default_env()
        .add_directive("h2=warn".parse().expect("well-formed"))
        .add_directive("hyper_util=warn".parse().expect("well-formed"))
        .add_directive("tycho_client=warn".parse().expect("well-formed"))
        .add_directive("tycho_simulation=warn".parse().expect("well-formed"));

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
