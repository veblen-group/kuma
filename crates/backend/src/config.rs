use color_eyre::eyre::Result;
use figment::{
    providers::{Env, Format, Yaml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Config {
    pub fn load() -> Result<Self> {
        let config: Config = Figment::new()
            .merge(Yaml::file("kuma.yaml"))
            .merge(Env::prefixed("KUMA_"))
            .extract()?;

        Ok(config)
    }
}
