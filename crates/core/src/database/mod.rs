use color_eyre::eyre::{self, OptionExt as _, Result, eyre};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{str::FromStr as _, sync::Arc};
use tracing::info;
use tycho_common::models::token::Token;

use crate::{
    chain::Chain,
    config::{DatabaseConfig, TokenAddressesForChain},
};

pub use signals::*;
pub use spot_prices::*;

mod signals;
mod spot_prices;

#[derive(Debug, Clone)]
pub struct Handle {
    pool: Arc<PgPool>,
    token_configs: Arc<TokenAddressesForChain>,
}

impl Handle {
    pub fn from_config(
        config: DatabaseConfig,
        token_configs: Arc<TokenAddressesForChain>,
    ) -> Result<Self> {
        let url = format!(
            "postgres://{}:{}@{}:{}/{}",
            config.user, config.password, config.host, config.port, config.dbname
        );
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.connection_timeout())
            .idle_timeout(config.idle_timeout())
            .connect_lazy(&url)
            .map_err(|e| eyre!("Failed to connect to database: {}", e))?;

        info!(
            "Connected to database with {} max connections",
            config.max_connections
        );

        let handle = Handle {
            pool: Arc::new(pool),
            token_configs,
        };

        Ok(handle)
    }
    #[allow(dead_code)]
    pub fn pool(&self) -> Arc<PgPool> {
        Arc::clone(&self.pool)
    }

    pub fn spot_price_repository(&self) -> SpotPriceRepository {
        SpotPriceRepository::new(Arc::clone(&self.pool), Arc::clone(&self.token_configs))
    }

    pub fn signal_repository(&self) -> SignalRepository {
        SignalRepository::new(Arc::clone(&self.pool), Arc::clone(&self.token_configs))
    }
}

fn try_token_from_chain_symbol(
    symbol: &str,
    chain: &Chain,
    token_configs: &TokenAddressesForChain,
) -> eyre::Result<Token> {
    let token = token_configs[chain]
        .values()
        .find(|token| token.symbol == symbol)
        .ok_or_eyre("token config not found for addr in db")?
        .clone();

    Ok(token)
}

fn try_chain_from_str(name: &str, token_configs: &TokenAddressesForChain) -> eyre::Result<Chain> {
    let chain_name = tycho_common::models::Chain::from_str(name)
        .map_err(|err| eyre!("failed to parse chain name: {err}"))?;
    let chain = token_configs
        .keys()
        .find(|c| c.name == chain_name)
        .ok_or_eyre("chain not configured")?
        .clone();

    Ok(chain)
}
