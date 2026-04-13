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
