use std::{str::FromStr as _, sync::Arc};

use color_eyre::eyre::{self, OptionExt, eyre};
use num_bigint::BigUint;
use sqlx::PgPool;
use tracing::{info, instrument};
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
    token_a_address: String,
    token_b_symbol: String,
    token_b_address: String,
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

    #[instrument(skip(self, spot_price))]
    #[allow(dead_code)]
    pub async fn insert(&self, spot_price: &SpotPrices) -> eyre::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO spot_prices (
                token_a_symbol, token_a_address,
                token_b_symbol, token_b_address,
                block_height, min_price, max_price, pool_id, chain
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            spot_price.token_a_symbol,
            spot_price.token_a_address,
            spot_price.token_b_symbol,
            spot_price.token_b_address,
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

    #[instrument(skip(self))]
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
                token_a_symbol, token_a_address,
                token_b_symbol, token_b_address,
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

    #[instrument(skip(self))]
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
                token_a_symbol, token_a_address,
                token_b_symbol, token_b_address,
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
            .map(|r| serde_json::from_value(r))
            .collect())
    }

    #[instrument(skip(self))]
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
    let price =
        BigUint::from_str(&row.price).map_err(|err| eyre!("failed to decode spot price: {err}"))?;
    let block_height = row.block_height;

    let chain_name = tycho_common::models::Chain::from_str(&row.chain)
        .map_err(|err| eyre!("failed to parse chain name: {err}"))?;
    let chain = token_configs
        .keys()
        .find(|c| c.name == chain_name)
        .ok_or_eyre("chain not configured")?
        .clone();

    let (token_a, token_b) = try_tokens_from_row(row, &chain, token_configs)
        .map_err(|err| eyre!("failed to get tokens from db: {err:}"))?;

    Ok(SpotPrices {
        pair: Pair::new(token_a, token_b),
        block_height,
        price,
        pool_id,
        chain,
    })
}

fn try_tokens_from_row(
    row: SpotPriceRow,
    chain: &Chain,
    token_configs: &TokenAddressesForChain,
) -> eyre::Result<(Token, Token)> {
    let token_a_addr = tycho_common::Bytes::from_str(&row.token_a_address)
        .map_err(|err| eyre!("failed to parse token a address bytes from db: {err}"))?;

    let token_a = token_configs[chain]
        .get(&token_a_addr)
        .ok_or_eyre("token a config not found for addr in db")?
        .clone();

    if token_a.symbol != row.token_a_symbol {
        eyre::bail!("token a config symbol doesn't match db value");
    }

    let token_b_addr = tycho_common::Bytes::from_str(&row.token_b_address)
        .map_err(|err| eyre!("failed to parse token b address bytes from db: {err}"))?;

    let token_b = token_configs[chain]
        .get(&token_b_addr)
        .ok_or_eyre("token b config not found for addr in db")?
        .clone();

    if token_b.symbol != row.token_b_symbol {
        eyre::bail!("token b config symbol doesn't match db value");
    }

    Ok((token_a, token_b))
}
