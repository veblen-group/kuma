use color_eyre::eyre::{self, OptionExt as _, Result, eyre};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::sync::Arc;
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

pub struct DatabaseBuilder {
    pub config: DatabaseConfig,
}

impl DatabaseBuilder {
    pub async fn build(self) -> Result<DatabaseHandle> {
        let pool = PgPoolOptions::new()
            .max_connections(self.config.max_connections)
            .acquire_timeout(self.config.connection_timeout())
            .idle_timeout(self.config.idle_timeout())
            .connect(&self.config.url)
            .await
            .map_err(|e| eyre!("Failed to connect to database: {}", e))?;

        info!(
            "Connected to database with {} max connections",
            self.config.max_connections
        );

        let handle = DatabaseHandle {
            pool: Arc::new(pool),
        };

        Ok(handle)
    }
}

pub struct DatabaseHandle {
    pool: Arc<PgPool>,
}

impl Clone for DatabaseHandle {
    fn clone(&self) -> Self {
        Self {
            pool: Arc::clone(&self.pool),
        }
    }
}

impl DatabaseHandle {
    #[allow(dead_code)]
    pub fn pool(&self) -> Arc<PgPool> {
        Arc::clone(&self.pool)
    }

    pub fn spot_price_repository(
        &self,
        token_configs: Arc<TokenAddressesForChain>,
    ) -> SpotPriceRepository {
        SpotPriceRepository::new(Arc::clone(&self.pool), token_configs)
    }

    pub fn signal_repository(
        &self,
        token_configs: Arc<TokenAddressesForChain>,
    ) -> SignalRepository {
        SignalRepository::new(Arc::clone(&self.pool), token_configs)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use proptest::prelude::*;

    #[tokio::test]
    async fn test_database_builder_default() {
        use crate::config::DatabaseConfig;
        let config = DatabaseConfig {
            url: "postgres://test@localhost/test".to_string(),
            max_connections: 10,
            connection_timeout_secs: 30,
            idle_timeout_secs: 600,
        };
        let builder = DatabaseBuilder { config };
        assert_eq!(builder.config.max_connections, 10);
        assert_eq!(builder.config.connection_timeout(), Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_database_builder_with_custom_values() {
        use crate::config::DatabaseConfig;
        let config = DatabaseConfig {
            url: "postgres://test@localhost/test".to_string(),
            max_connections: 5,
            connection_timeout_secs: 60,
            idle_timeout_secs: 1200,
        };
        let builder = DatabaseBuilder { config };

        assert_eq!(builder.config.url, "postgres://test@localhost/test");
        assert_eq!(builder.config.max_connections, 5);
    }

    proptest! {
        #[test]
        fn test_database_builder_properties(
            max_connections in 1u32..=100,
        ) {
            use crate::config::DatabaseConfig;
            let config = DatabaseConfig {
                url: "postgres://test@localhost/test".to_string(),
                max_connections,
                connection_timeout_secs: 30,
                idle_timeout_secs: 600,
            };
            let builder = DatabaseBuilder {
                config,
            };

            prop_assert_eq!(builder.config.max_connections, max_connections);
        }
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
