use figment::{providers::Env, Figment};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub binance_api_key: String,
    pub log_level: String,
}

pub fn get() -> Result<Config, figment::Error> {
    let figment = Figment::new().merge(Env::prefixed("KUMA_"));

    figment.extract::<Config>()
}
