use color_eyre::eyre;
use figment::{Figment, providers::Env};
use num_bigint::BigUint;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub binance_api_key: String,
    pub log_level: String,
}

pub fn get() -> Result<Config, figment::Error> {
    // TODO: clean up config
    let figment = Figment::new().merge(Env::prefixed("KUMA_"));

    figment.extract::<Config>()
}
