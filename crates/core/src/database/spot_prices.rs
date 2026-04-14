//! Spot price persistence.
//!
//! `SpotPriceRepository` writes and reads `SpotPrices` rows — one row per
//! `(chain, pair, block_height)`. Rows are deduplicated: `insert` is a no-op if
//! the prices and pool IDs are identical to the most recent row for that pair.
//!
//! ## Write path (strategy worker → DB)
//!
//! - `write_slow_spot_prices` — called on every slow chain block; writes the primary pair
//!   prices plus up to three auxiliary pairs (A/USDC, B/USDC, ETH/USDC).
//! - `write_fast_spot_prices` — called on every fast chain block; writes only the A/B pair.
//!
//! Both are fire-and-forget: callers push them into a `FuturesUnordered` and don't await.
//!
//! ## Read path (backend API + signal FK resolution)
//!
//! - `get_by_ids` — batch fetch by primary key; used when reconstructing signals from DB
//!   (signal rows store spot price IDs as FKs).
//! - `get_by_symbols` / `count_by_symbols` — paginated list for the HTTP API.

use std::{collections::HashMap, sync::Arc};

use color_eyre::eyre::{self, Context as _, eyre};
use sqlx::{
    PgPool,
    types::chrono::{DateTime, Utc},
};

use crate::{
    config::TokenAddressesForChain,
    spot_prices::SpotPrices,
    state::{PoolId, pair::Pair},
};

use super::{try_chain_from_str, try_token_from_chain_symbol};

struct SpotPricesRow {
    id: i64,
    created_at: DateTime<Utc>,
    chain: String,
    block_height: i64,
    min_pool_id: String,
    max_pool_id: String,
    min_price: f64,
    max_price: f64,
    token_a_symbol: String,
    token_b_symbol: String,
}

impl SpotPricesRow {
    fn try_into_spot_prices(
        self,
        token_configs: &TokenAddressesForChain,
    ) -> eyre::Result<SpotPrices> {
        let chain = try_chain_from_str(&self.chain, token_configs)?;
        let token_a = try_token_from_chain_symbol(&self.token_a_symbol, &chain, token_configs)
            .map_err(|e| eyre!("failed to parse token a from db: {e:}"))?;
        let token_b = try_token_from_chain_symbol(&self.token_b_symbol, &chain, token_configs)
            .map_err(|e| eyre!("failed to parse token b from db: {e:}"))?;
        Ok(SpotPrices {
            pair: Pair::new(token_a, token_b),
            block_height: self.block_height as u64,
            min_price: self.min_price,
            max_price: self.max_price,
            min_pool_id: PoolId::from(self.min_pool_id.as_str()),
            max_pool_id: PoolId::from(self.max_pool_id.as_str()),
            chain,
        })
    }
}

#[derive(Clone)]
pub struct SpotPriceRepository {
    pool: Arc<PgPool>,
    token_configs: Arc<TokenAddressesForChain>,
}

impl SpotPriceRepository {
    pub(super) fn new(pool: Arc<PgPool>, token_configs: Arc<TokenAddressesForChain>) -> Self {
        Self {
            pool,
            token_configs,
        }
    }

    /// Insert slow-chain spot prices into the database — fire and forget.
    /// Spot price FK linkage is resolved at signal insert time via a SQL CTE.
    pub async fn write_slow_spot_prices(
        &self,
        prices_a_b: SpotPrices,
        prices_a_usdc: Option<SpotPrices>,
        prices_b_usdc: Option<SpotPrices>,
        prices_eth_usdc: Option<SpotPrices>,
    ) -> eyre::Result<()> {
        let pair = prices_a_b.pair.clone();
        self.insert(prices_a_b)
            .await
            .wrap_err_with(|| format!("failed to write spot prices to db for {pair}"))?;

        if let Some(prices) = prices_a_usdc {
            let pair = prices.pair.clone();
            self.insert(prices)
                .await
                .wrap_err_with(|| eyre!("failed to write spot prices to db for {pair}"))?;
        }

        if let Some(prices) = prices_b_usdc {
            let pair = prices.pair.clone();
            self.insert(prices)
                .await
                .wrap_err_with(|| eyre!("failed to write spot prices to db for {pair}"))?;
        }

        if let Some(prices) = prices_eth_usdc {
            let pair = prices.pair.clone();
            self.insert(prices)
                .await
                .wrap_err_with(|| eyre!("failed to write ETH-USDC spot prices to db for {pair}"))?;
        }

        Ok(())
    }

    /// Insert fast-chain A/B spot prices — fire and forget.
    pub async fn write_fast_spot_prices(&self, prices: SpotPrices) -> eyre::Result<()> {
        let pair = prices.pair.clone();
        self.insert(prices)
            .await
            .wrap_err_with(|| format!("failed to write spot prices to db for {pair}"))?;
        Ok(())
    }

    /// Insert a spot price row, skipping if prices and pool IDs are unchanged from the last row.
    ///
    /// The dedup check compares `(min_price, max_price, min_pool_id, max_pool_id)` against the
    /// most recent row for the same `(chain, token_a, token_b)`. Identical consecutive readings
    /// produce no row — this keeps the table from accumulating redundant data during quiet blocks.
    pub async fn insert(&self, spot_prices: SpotPrices) -> eyre::Result<()> {
        sqlx::query!(
            r#"
            WITH last AS (
                SELECT min_price, max_price, min_pool_id, max_pool_id
                FROM spot_prices
                WHERE chain = $7
                  AND token_a_symbol = $1
                  AND token_b_symbol = $2
                ORDER BY block_height DESC
                LIMIT 1
            )
            INSERT INTO spot_prices (
                token_a_symbol, token_b_symbol,
                min_price, max_price,
                min_pool_id, max_pool_id,
                chain, block_height
            )
            SELECT $1, $2, $3::float8, $4::float8, $5::text, $6::text, $7, $8
            WHERE NOT EXISTS (
                SELECT 1 FROM last
                WHERE last.min_price = $3::float8
                  AND last.max_price = $4::float8
                  AND last.min_pool_id = $5::text
                  AND last.max_pool_id = $6::text
            )
            "#,
            spot_prices.pair.token_a().symbol,
            spot_prices.pair.token_b().symbol,
            spot_prices.min_price,
            spot_prices.max_price,
            spot_prices.min_pool_id.to_string(),
            spot_prices.max_pool_id.to_string(),
            spot_prices.chain.name.to_string(),
            spot_prices.block_height as i64,
        )
        .execute(self.pool.as_ref())
        .await?;

        Ok(())
    }

    /// Fetch multiple spot price rows by their IDs in a single query.
    /// Returns a map of `id → SpotPrices` for efficient lookup when
    /// reconstructing signals from `SignalRow` spot price FK IDs.
    pub async fn get_by_ids(&self, ids: &[i64]) -> eyre::Result<HashMap<i64, (DateTime<Utc>, SpotPrices)>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query_as!(
            SpotPricesRow,
            r#"
            SELECT
                id,
                created_at as "created_at!",
                token_a_symbol, token_b_symbol,
                min_price, max_price,
                min_pool_id, max_pool_id,
                chain, block_height
            FROM spot_prices
            WHERE id = ANY($1)
            "#,
            ids
        )
        .fetch_all(self.pool.as_ref())
        .await?;

        rows.into_iter()
            .map(|r| {
                let id = r.id;
                let created_at = r.created_at;
                r.try_into_spot_prices(&self.token_configs)
                    .map(|sp| (id, (created_at, sp)))
            })
            .collect()
    }

    pub async fn count_by_symbols(
        &self,
        token_a_symbol: &str,
        token_b_symbol: &str,
    ) -> eyre::Result<u64> {
        let (token_a_symbol, token_b_symbol) = if token_a_symbol < token_b_symbol {
            (token_a_symbol, token_b_symbol)
        } else {
            (token_b_symbol, token_a_symbol)
        };

        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT
                COUNT(*) as count
            FROM spot_prices
            WHERE token_a_symbol = $1 AND token_b_symbol = $2
            "#,
        )
        .bind(token_a_symbol)
        .bind(token_b_symbol)
        .fetch_one(self.pool.as_ref())
        .await?;

        Ok(count as u64)
    }

    /// Fetch paginated spot price rows for a token pair, filtered to a specific set of chains.
    ///
    /// Identical to [`get_by_symbols`] but adds a `chain = ANY(chains)` predicate so only
    /// rows from the requested chains are returned. Uses runtime `sqlx::query_as` with a
    /// local `FromRow` struct to avoid needing an offline query cache entry.
    ///
    /// Token symbols are normalised to alphabetical order before querying (same convention
    /// as the rest of the repository) so callers may pass them in either order.
    pub async fn get_by_symbols_and_chains(
        &self,
        token_a_symbol: &str,
        token_b_symbol: &str,
        chains: &[String],
        limit: u32,
        offset: u32,
    ) -> eyre::Result<Vec<(i64, DateTime<Utc>, SpotPrices)>> {
        let (token_a_symbol, token_b_symbol) = if token_a_symbol < token_b_symbol {
            (token_a_symbol, token_b_symbol)
        } else {
            (token_b_symbol, token_a_symbol)
        };

        // Use a local FromRow struct so we don't need a cached query entry.
        // created_at is nullable in the schema (DEFAULT NOW()), so Option here.
        #[derive(sqlx::FromRow)]
        struct Row {
            id: i64,
            created_at: Option<DateTime<Utc>>,
            chain: String,
            block_height: i64,
            min_pool_id: String,
            max_pool_id: String,
            min_price: f64,
            max_price: f64,
            token_a_symbol: String,
            token_b_symbol: String,
        }

        let rows: Vec<Row> = sqlx::query_as(
            r#"
            SELECT
                id, created_at,
                token_a_symbol, token_b_symbol,
                min_price, max_price,
                min_pool_id, max_pool_id,
                chain, block_height
            FROM spot_prices
            WHERE token_a_symbol = $1 AND token_b_symbol = $2
              AND chain = ANY($3)
            ORDER BY created_at DESC
            LIMIT $4 OFFSET $5
            "#,
        )
        .bind(token_a_symbol)
        .bind(token_b_symbol)
        .bind(chains)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(self.pool.as_ref())
        .await?;

        rows.into_iter()
            .map(|r| {
                let id = r.id;
                let created_at = r.created_at.unwrap_or_default();
                SpotPricesRow {
                    id: r.id,
                    created_at,
                    chain: r.chain,
                    block_height: r.block_height,
                    min_pool_id: r.min_pool_id,
                    max_pool_id: r.max_pool_id,
                    min_price: r.min_price,
                    max_price: r.max_price,
                    token_a_symbol: r.token_a_symbol,
                    token_b_symbol: r.token_b_symbol,
                }
                .try_into_spot_prices(&self.token_configs)
                .map(|sp| (id, created_at, sp))
            })
            .collect()
    }

    /// Fetch paginated spot prices for a pair ordered by `created_at DESC`.
    /// Token symbols are sorted alphabetically before querying so that
    /// `(USDC, WETH)` and `(WETH, USDC)` resolve to the same rows.
    pub async fn get_by_symbols(
        &self,
        token_a_symbol: &str,
        token_b_symbol: &str,
        limit: u32,
        offset: u32,
    ) -> eyre::Result<Vec<(i64, DateTime<Utc>, SpotPrices)>> {
        let (token_a_symbol, token_b_symbol) = if token_a_symbol < token_b_symbol {
            (token_a_symbol, token_b_symbol)
        } else {
            (token_b_symbol, token_a_symbol)
        };

        let rows = sqlx::query_as!(
            SpotPricesRow,
            r#"
            SELECT
                id,
                created_at as "created_at!",
                token_a_symbol, token_b_symbol,
                min_price, max_price,
                min_pool_id, max_pool_id,
                chain, block_height
            FROM spot_prices
            WHERE token_a_symbol = $1 AND token_b_symbol = $2
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#,
            token_a_symbol,
            token_b_symbol,
            limit as i64,
            offset as i64,
        )
        .fetch_all(self.pool.as_ref())
        .await?;

        rows.into_iter()
            .map(|r| {
                let id = r.id;
                let created_at = r.created_at;
                r.try_into_spot_prices(&self.token_configs)
                    .map(|sp| (id, created_at, sp))
            })
            .collect()
    }
}
