use color_eyre::eyre::{eyre, Result};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::Arc;
use tracing::info;

use crate::config::DatabaseConfig;

pub use repositories::*;

mod repositories;

pub struct DatabaseBuilder {
    pub(crate) config: DatabaseConfig,
}

impl DatabaseBuilder {
    pub fn new(config: DatabaseConfig) -> Self {
        Self { config }
    }

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
    pub fn pool(&self) -> Arc<PgPool> {
        Arc::clone(&self.pool)
    }

    pub fn spot_price_repository(&self) -> SpotPriceRepository {
        SpotPriceRepository::new(Arc::clone(&self.pool))
    }

    pub fn arbitrage_signal_repository(&self) -> ArbitrageSignalRepository {
        ArbitrageSignalRepository::new(Arc::clone(&self.pool))
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
        let builder = DatabaseBuilder::new(config);
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
        let builder = DatabaseBuilder::new(config);

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
            let builder = DatabaseBuilder::new(config);

            prop_assert_eq!(builder.config.max_connections, max_connections);
        }
    }
}
