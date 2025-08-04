use std::{str::FromStr as _, sync::Arc};

use color_eyre::eyre::{self, eyre};
use num_bigint::BigUint;
use sqlx::PgPool;

use crate::{
    config::TokenAddressesForChain,
    spot_prices::SpotPrices,
    state::{PoolId, pair::Pair},
};

use super::{try_chain_from_str, try_token_from_chain_symbol};

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

    #[allow(dead_code)]
    pub async fn insert(&self, spot_price: &SpotPrices) -> eyre::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO spot_prices (
                token_a_symbol,
                token_b_symbol,
                block_height, min_price, max_price, pool_id, chain
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
            spot_price.pair.token_a().symbol,
            spot_price.pair.token_b().symbol,
            spot_price.block_height as i64,
            spot_price.min_price.to_string(),
            spot_price.max_price.to_string(),
            spot_price.pool_id.to_string(),
            spot_price.chain.name.to_string(),
        )
        .execute(self.pool.as_ref())
        .await?;

        Ok(())
    }

    pub async fn count_by_symbols(
        &self,
        token_a_symbol: &str,
        token_b_symbol: &str,
    ) -> eyre::Result<u64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) as count
            FROM spot_prices
            WHERE ((token_a_symbol = $1 AND token_b_symbol = $2)
                OR (token_a_symbol = $2 AND token_b_symbol = $1))
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
    ) -> eyre::Result<Vec<SpotPrices>> {
        let rows = sqlx::query_as!(
            SpotPriceRow,
            r#"
            SELECT
                token_a_symbol,
                token_b_symbol,
                block_height, min_price, max_price, pool_id, chain
            FROM spot_prices
            WHERE ((token_a_symbol = $1 AND token_b_symbol = $2)
                OR (token_a_symbol = $2 AND token_b_symbol = $1))
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
            .map(|r| try_spot_price_from_row(r, &self.token_configs))
            .collect()
    }
}

struct SpotPriceRow {
    chain: String,
    block_height: i64,
    pool_id: String,
    min_price: String,
    max_price: String,
    token_a_symbol: String,
    token_b_symbol: String,
}

fn try_spot_price_from_row(
    row: SpotPriceRow,
    token_configs: &TokenAddressesForChain,
) -> eyre::Result<SpotPrices> {
    let pool_id = PoolId::from(row.pool_id.as_str());

    let min_price = BigUint::from_str(&row.min_price)
        .map_err(|e| eyre!("failed to parse min price from db: {e}"))?;
    let max_price = BigUint::from_str(&row.max_price)
        .map_err(|e| eyre!("failed to parse max price from db: {e}"))?;

    let block_height = row.block_height as u64;

    let chain = try_chain_from_str(&row.chain, token_configs)?;

    let token_a = try_token_from_chain_symbol(&row.token_a_symbol, &chain, token_configs)
        .map_err(|e| eyre!("failed to parse token a from db: {e:}"))?;
    let token_b = try_token_from_chain_symbol(&row.token_b_symbol, &chain, token_configs)
        .map_err(|e| eyre!("failed to parse token b from db: {e:}"))?;

    Ok(SpotPrices {
        pair: Pair::new(token_a, token_b),
        block_height,
        min_price,
        max_price,
        pool_id,
        chain,
    })
}
