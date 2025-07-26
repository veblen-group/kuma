use color_eyre::eyre::{eyre, Result};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, oneshot},
    time::interval,
};
use tracing::{error, info, warn};

pub use repositories::*;

mod repositories;

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub connection_timeout: Duration,
    pub idle_timeout: Duration,
    pub health_check_interval: Duration,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://api_user:password@localhost:5432/api_db".to_string()
            }),
            max_connections: 10,
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600),
            health_check_interval: Duration::from_secs(30),
        }
    }
}

pub struct DatabaseBuilder {
    pub(crate) config: DatabaseConfig,
}

impl DatabaseBuilder {
    pub fn new() -> Self {
        Self {
            config: DatabaseConfig::default(),
        }
    }

    pub fn with_config(mut self, config: DatabaseConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_url(mut self, url: String) -> Self {
        self.config.url = url;
        self
    }

    pub fn with_max_connections(mut self, max_connections: u32) -> Self {
        self.config.max_connections = max_connections;
        self
    }

    pub async fn build(self) -> Result<(DatabaseWorker, DatabaseHandle)> {
        let pool = PgPoolOptions::new()
            .max_connections(self.config.max_connections)
            .acquire_timeout(self.config.connection_timeout)
            .idle_timeout(self.config.idle_timeout)
            .connect(&self.config.url)
            .await
            .map_err(|e| eyre!("Failed to connect to database: {}", e))?;

        info!(
            "Connected to database with {} max connections",
            self.config.max_connections
        );

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (health_tx, health_rx) = mpsc::unbounded_channel();

        let worker = DatabaseWorker {
            pool: Arc::new(pool.clone()),
            config: self.config.clone(),
            shutdown_rx,
            health_tx,
        };

        let handle = DatabaseHandle {
            pool: Arc::new(pool),
            shutdown_tx: Some(shutdown_tx),
            health_rx,
        };

        Ok((worker, handle))
    }
}

impl Default for DatabaseBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct DatabaseWorker {
    pool: Arc<PgPool>,
    config: DatabaseConfig,
    shutdown_rx: oneshot::Receiver<()>,
    health_tx: mpsc::UnboundedSender<DatabaseHealth>,
}

impl DatabaseWorker {
    pub async fn run(mut self) -> Result<()> {
        let mut health_interval = interval(self.config.health_check_interval);
        health_interval.tick().await; // Skip the first immediate tick

        loop {
            tokio::select! {
                _ = &mut self.shutdown_rx => {
                    info!("Database worker shutting down");
                    break;
                }
                _ = health_interval.tick() => {
                    let health = self.check_health().await;
                    let _ = self.health_tx.send(health); // Ignore send errors if no receivers
                }
            }
        }

        self.pool.close().await;
        Ok(())
    }

    async fn check_health(&self) -> DatabaseHealth {
        match sqlx::query("SELECT 1").execute(&*self.pool).await {
            Ok(_) => DatabaseHealth::Healthy,
            Err(e) => {
                error!("Database health check failed: {}", e);
                DatabaseHealth::Unhealthy(e.to_string())
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum DatabaseHealth {
    Healthy,
    Unhealthy(String),
}

pub struct DatabaseHandle {
    pool: Arc<PgPool>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    health_rx: mpsc::UnboundedReceiver<DatabaseHealth>,
}

impl Clone for DatabaseHandle {
    fn clone(&self) -> Self {
        let (_, health_rx) = mpsc::unbounded_channel();
        Self {
            pool: Arc::clone(&self.pool),
            shutdown_tx: None, // Only the original handle can shut down
            health_rx,
        }
    }
}

impl DatabaseHandle {
    pub fn pool(&self) -> Arc<PgPool> {
        Arc::clone(&self.pool)
    }

    pub async fn health(&mut self) -> Option<DatabaseHealth> {
        self.health_rx.recv().await
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            if tx.send(()).is_err() {
                warn!("Database worker already shut down");
            }
        }
        Ok(())
    }

    pub fn pair_price_repository(&self) -> PairPriceRepository {
        PairPriceRepository::new(Arc::clone(&self.pool))
    }

    pub fn arbitrage_signal_repository(&self) -> ArbitrageSignalRepository {
        ArbitrageSignalRepository::new(Arc::clone(&self.pool))
    }
}

impl Drop for DatabaseHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[tokio::test]
    async fn test_database_builder_default() {
        let builder = DatabaseBuilder::new();
        assert_eq!(builder.config.max_connections, 10);
        assert_eq!(builder.config.connection_timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_database_builder_with_config() {
        let config = DatabaseConfig {
            url: "postgres://test@localhost/test".to_string(),
            max_connections: 5,
            connection_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(300),
            health_check_interval: Duration::from_secs(60),
        };

        let builder = DatabaseBuilder::new().with_config(config.clone());
        assert_eq!(builder.config.url, config.url);
        assert_eq!(builder.config.max_connections, 5);
    }

    proptest! {
        #[test]
        fn test_database_config_properties(
            max_connections in 1u32..=100,
            connection_timeout_secs in 1u64..=300,
            idle_timeout_secs in 60u64..=3600,
            health_check_interval_secs in 10u64..=600,
        ) {
            let config = DatabaseConfig {
                url: "postgres://localhost/test".to_string(),
                max_connections,
                connection_timeout: Duration::from_secs(connection_timeout_secs),
                idle_timeout: Duration::from_secs(idle_timeout_secs),
                health_check_interval: Duration::from_secs(health_check_interval_secs),
            };

            let builder = DatabaseBuilder::new().with_config(config.clone());
            prop_assert_eq!(builder.config.max_connections, max_connections);
            prop_assert_eq!(builder.config.connection_timeout, Duration::from_secs(connection_timeout_secs));
            prop_assert_eq!(builder.config.idle_timeout, Duration::from_secs(idle_timeout_secs));
            prop_assert_eq!(builder.config.health_check_interval, Duration::from_secs(health_check_interval_secs));
        }
    }

    #[tokio::test]
    async fn test_database_handle_shutdown() {
        let config = DatabaseConfig {
            url: "postgres://test:password@localhost/nonexistent_db".to_string(),
            ..Default::default()
        };

        if let Ok((worker, mut handle)) = DatabaseBuilder::new().with_config(config).build().await {
            let worker_task = tokio::spawn(async move { worker.run().await });

            let shutdown_result = handle.shutdown().await;
            assert!(shutdown_result.is_ok());

            let _ = worker_task.await;
        }
    }
}
