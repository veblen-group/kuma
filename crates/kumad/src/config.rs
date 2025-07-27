use figment::{Figment, providers::Env};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub binance_api_key: String,
    pub log_level: String,
    pub core: kuma_core::config::Config,
}

pub fn get() -> Result<Config, figment::Error> {
    // TODO: add core config
    let figment = Figment::new().merge(Env::prefixed("KUMA_"));

    figment.extract::<Config>()
}
