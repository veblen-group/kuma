//! PostgreSQL persistence layer.
//!
//! `Handle` is a cheap-to-clone wrapper around a `sqlx::PgPool`. It vends short-lived
//! repository objects for each entity type:
//!
//! - [`SpotPriceRepository`] — write slow/fast chain spot prices
//! - [`SignalRepository`] — insert generated arbitrage signals
//! - [`TradeRepository`] — insert trade results
//!
//! ## Fire-and-forget write pattern
//!
//! All writes in the strategy and execution workers are pushed into a `FuturesUnordered`
//! and polled in the `select!` loop. Errors are logged but never propagate — a failed DB
//! write does not cancel a trade or stall signal generation.
//!
//! ## Signal FK resolution
//!
//! Signal rows reference spot price rows by foreign key. Rather than coordinating futures,
//! the signal INSERT uses a SQL CTE to look up `spot_prices` rows by `(chain, pair, block_height)`
//! at write time — spot-price and signal futures can be fired concurrently.
//!
//! See `docs/database.md` for the schema overview.

use color_eyre::eyre::{self, OptionExt as _, Result, eyre};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{str::FromStr as _, sync::Arc};
use tracing::info;
use tycho_simulation::tycho_common::{self, models::token::Token};

use crate::{
    chain::Chain,
    config::{DatabaseConfig, TokenAddressesForChain},
};

pub use signals::*;
pub use spot_prices::*;
pub use trade::*;

mod signals;
mod spot_prices;
mod trade;

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

    pub fn trade_repository(&self) -> TradeRepository {
        TradeRepository::new(Arc::clone(&self.pool), Arc::clone(&self.token_configs))
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
