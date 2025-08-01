use std::{str::FromStr as _, sync::Arc};

use color_eyre::eyre::{self, OptionExt, eyre};
use num_bigint::BigUint;
use sqlx::PgPool;
use tycho_common::models::token::Token;

use crate::{
    chain::Chain,
    config::TokenAddressesForChain,
    spot_prices::SpotPrices,
    state::{PoolId, pair::Pair},
};

struct SpotPriceRow {
    chain: String,
    block_height: u64,
    pool_id: String,
    min_price: String,
    max_price: String,
    token_a_symbol: String,
    token_b_symbol: String,
}

#[derive(Clone)]
pub struct SpotPriceRepository {
    pool: Arc<PgPool>,
    token_configs: Arc<TokenAddressesForChain>,
}

impl SpotPriceRepository {
    pub fn new(pool: Arc<PgPool>, token_configs: Arc<TokenAddressesForChain>) -> Self {
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
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            spot_price.token_a_symbol,
            spot_price.token_b_symbol,
            spot_price.block_height,
            spot_price.min_price,
            spot_price.max_price,
            spot_price.pool_id,
            spot_price.chain
        )
        .execute(self.pool)
        .await?;

        Ok(())
    }

    pub async fn count_by_pair(&self, pair: Pair) -> eyre::Result<u64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) as count
            FROM spot_prices
            WHERE token_a_symbol = $1 AND token_b_symbol = $2
            "#,
        )
        .bind(&pair.token_a().symbol)
        .bind(&pair.token_b().symbol)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count as u64)
    }

    pub async fn get_spot_prices(
        &self,
        pair: Pair,
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
            WHERE (token_a_symbol = $1 AND token_b_symbol = $2)
            ORDER BY block_height
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(&pair.token_a())
        .bind(&pair.token_b())
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&*self.pool)
        .await?;

        rows.into_iter()
            .map(|r| try_spot_price_from_row(r, &self.token_configs))
            .collect()
    }

    pub async fn get_spot_prices_by_chain(
        &self,
        chain: &str,
        limit: u32,
        offset: u32,
    ) -> eyre::Result<Vec<SpotPrices>> {
        let rows = sqlx::query_as!(
            SpotPriceRow,
            r#"
            SELECT
                token_a_symbol,
                token_b_symbol,
                block_height, price, pool_id, chain
            FROM spot_prices
            WHERE chain = $1
            ORDER BY block_height DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| try_spot_price_from_row(r, &self.token_configs))
            .collect())
    }

    pub async fn count_by_chain(&self, chain: &str) -> eyre::Result<u64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) as count
            FROM spot_prices
            WHERE chain = $1
            "#,
        )
        .bind(chain)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count as u64)
    }
}

pub(crate) fn try_spot_price_from_row(
    row: SpotPriceRow,
    token_configs: &TokenAddressesForChain,
) -> eyre::Result<SpotPrices> {
    let pool_id = PoolId::from(row.pool_id.as_str());

    let min_price = BigUint::from_str(&row.min_price)
        .map_err(|err| eyre!("failed to decode spot price: {err}"))?;
    let max_price = BigUint::from_str(&row.max_price)
        .map_err(|err| eyre!("failed to decode max price: {err}"))?;

    let block_height = row.block_height;

    let chain_name = tycho_common::models::Chain::from_str(&row.chain)
        .map_err(|err| eyre!("failed to parse chain name: {err}"))?;
    let chain = token_configs
        .keys()
        .find(|c| c.name == chain_name)
        .ok_or_eyre("chain not configured")?
        .clone();

    let token_a = try_token_from_chain_symbol(&row.token_a_symbol, &chain, token_configs)
        .map_err(|err| eyre!("failed to get token a from db: {err:}"))?;
    let token_b = try_token_from_chain_symbol(&row.token_b_symbol, &chain, token_configs)
        .map_err(|err| eyre!("failed to get token b from db: {err:}"))?;

    Ok(SpotPrices {
        pair: Pair::new(token_a, token_b),
        block_height,
        min_price,
        max_price,
        pool_id,
        chain,
    })
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
