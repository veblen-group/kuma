use crate::{config::TokenAddressesForChain, signals::CrossChainSingleHop, spot_price::SpotPrice};
use color_eyre::eyre::Result;
use sqlx::{PgPool, Row, types::Json};
use std::sync::Arc;
use tracing::{info, instrument};

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
    pub async fn insert(&self, spot_price: &SpotPrice) -> Result<()> {
        sqlx::query_as!(
            SpotPrice,
            r#"
            INSERT INTO spot_prices (
                token_a_symbol, token_a_address,
                token_b_symbol, token_b_address,
                block_height, price, pool_id, chain
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            // TODO: enter spot price fields
        )
        .fetch_one(self.pool)
        .await?;

        info!("Inserted spot price for block {}", spot_price.block_height);
        Ok(())
    }

    #[instrument(skip(self))]
    #[allow(dead_code)]
    pub async fn get_latest_by_pool(&self, pool_id: &str) -> Result<Option<SpotPrice>> {
        let row = sqlx::query(
            r#"
            SELECT
                token_a_symbol, token_a_address,
                token_b_symbol, token_b_address,
                block_height, price, pool_id, chain
            FROM spot_prices
            WHERE pool_id = $1
            ORDER BY block_height DESC
            LIMIT 1
            "#,
        )
        .bind(pool_id)
        .fetch_optional(&*self.pool)
        .await?;

        row.map(|r| SpotPrice::try_from_row(r, &self.token_configs))
    }

    #[instrument(skip(self))]
    #[allow(dead_code)]
    pub async fn get_by_block_range(
        &self,
        pool_id: &str,
        start_block: u64,
        end_block: u64,
    ) -> Result<Vec<SpotPrice>> {
        let rows = sqlx::query(
            r#"
            SELECT
                token_a_symbol, token_a_address,
                token_b_symbol, token_b_address,
                block_height, price, pool_id, chain
            FROM spot_prices
            WHERE pool_id = $1 AND block_height BETWEEN $2 AND $3
            ORDER BY block_height ASC
            "#,
        )
        .bind(pool_id)
        .bind(start_block as i64)
        .bind(end_block as i64)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| SpotPrice::try_from_row(row, self.token_configs))
            .collect())
    }

    #[instrument(skip(self))]
    pub async fn count_by_block_height(
        &self,
        block_height: u64,
        pair_filter: Option<&str>,
    ) -> Result<u64> {
        let count: i64 = match pair_filter {
            Some(pair) => {
                let parts: Vec<&str> = pair.split('-').collect();
                if parts.len() != 2 {
                    return Ok(0);
                }
                let token_a = parts[0];
                let token_b = parts[1];

                sqlx::query_scalar(
                    r#"
                    SELECT COUNT(*) as count
                    FROM spot_prices
                    WHERE block_height = $1
                    AND ((token_a_symbol = $2 AND token_b_symbol = $3)
                         OR (token_a_symbol = $3 AND token_b_symbol = $2))
                    "#,
                )
                .bind(block_height as i64)
                .bind(token_a)
                .bind(token_b)
                .fetch_one(&*self.pool)
                .await?
            }
            None => {
                sqlx::query_scalar(
                    r#"
                    SELECT COUNT(*) as count
                    FROM spot_prices
                    WHERE block_height = $1
                    "#,
                )
                .bind(block_height as i64)
                .fetch_one(&*self.pool)
                .await?
            }
        };

        Ok(count as u64)
    }

    pub async fn get_by_block_height(
        &self,
        block_height: u64,
        pair_filter: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<SpotPrice>> {
        let rows = match pair_filter {
            Some(pair) => {
                let parts: Vec<&str> = pair.split('-').collect();
                if parts.len() != 2 {
                    return Ok(vec![]);
                }
                let token_a = parts[0];
                let token_b = parts[1];

                sqlx::query(
                    r#"
                    SELECT
                        token_a_symbol, token_a_address,
                        token_b_symbol, token_b_address,
                        block_height, price, pool_id, chain
                    FROM spot_prices
                    WHERE block_height = $1
                    AND ((token_a_symbol = $2 AND token_b_symbol = $3)
                         OR (token_a_symbol = $3 AND token_b_symbol = $2))
                    ORDER BY pool_id
                    LIMIT $4 OFFSET $5
                    "#,
                )
                .bind(block_height as i64)
                .bind(token_a)
                .bind(token_b)
                .bind(limit as i64)
                .bind(offset as i64)
                .fetch_all(&*self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    r#"
                    SELECT
                        token_a_symbol, token_a_address,
                        token_b_symbol, token_b_address,
                        block_height, price, pool_id, chain
                    FROM spot_prices
                    WHERE block_height = $1
                    ORDER BY pool_id
                    LIMIT $2 OFFSET $3
                    "#,
                )
                .bind(block_height as i64)
                .bind(limit as i64)
                .bind(offset as i64)
                .fetch_all(&*self.pool)
                .await?
            }
        };

        Ok(rows
            .into_iter()
            .map(|r| SpotPrice {
                pair: crate::models::Pair {
                    token_a: crate::models::Token {
                        symbol: r.get("token_a_symbol"),
                        address: r.get("token_a_address"),
                    },
                    token_b: crate::models::Token {
                        symbol: r.get("token_b_symbol"),
                        address: r.get("token_b_address"),
                    },
                },
                block_height: r.get::<i64, _>("block_height") as u64,
                price: r.get("price"),
                pool_id: r.get("pool_id"),
                chain: r.get("chain"),
            })
            .collect())
    }

    #[instrument(skip(self))]
    pub async fn get_by_chain(
        &self,
        chain: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<SpotPrice>> {
        let rows = sqlx::query_as!(
            Row,
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
    pub async fn count_by_chain(&self, chain: &str) -> Result<u64> {
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

#[derive(Clone)]
pub struct ArbitrageSignalRepository {
    pool: Arc<PgPool>,
    tokens_config: Arc<TokenAddressesForChain>,
}

impl ArbitrageSignalRepository {
    pub fn new(pool: Arc<PgPool>, tokens_config: Arc<TokenAddressesForChain>) -> Self {
        Self {
            pool,
            tokens_config,
        }
    }

    #[instrument(skip(self, signal))]
    #[allow(dead_code)]
    pub async fn insert(&self, signal: &ArbitrageSignal) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO arbitrage_signals (
                block_height, slow_chain, slow_pair_token_a_symbol, slow_pair_token_a_address,
                slow_pair_token_b_symbol, slow_pair_token_b_address, slow_pool_id,
                fast_chain, fast_pair_token_a_symbol, fast_pair_token_a_address,
                fast_pair_token_b_symbol, fast_pair_token_b_address, fast_pool_id,
                slow_swap_token_in_symbol, slow_swap_token_in_address,
                slow_swap_token_out_symbol, slow_swap_token_out_address,
                slow_swap_amount_in, slow_swap_amount_out,
                fast_swap_token_in_symbol, fast_swap_token_in_address,
                fast_swap_token_out_symbol, fast_swap_token_out_address,
                fast_swap_amount_in, fast_swap_amount_out,
                surplus_a, surplus_b, expected_profit_a, expected_profit_b, max_slippage_bps
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25,
                $26, $27, $28, $29, $30
            )
            "#,
        )
        .bind(signal.block_height as i64)
        .bind(&signal.slow_chain)
        .bind(&signal.slow_pair.token_a.symbol)
        .bind(&signal.slow_pair.token_a.address)
        .bind(&signal.slow_pair.token_b.symbol)
        .bind(&signal.slow_pair.token_b.address)
        .bind(&signal.slow_pool_id)
        .bind(&signal.fast_chain)
        .bind(&signal.fast_pair.token_a.symbol)
        .bind(&signal.fast_pair.token_a.address)
        .bind(&signal.fast_pair.token_b.symbol)
        .bind(&signal.fast_pair.token_b.address)
        .bind(&signal.fast_pool_id)
        .bind(&signal.slow_swap.token_in.symbol)
        .bind(&signal.slow_swap.token_in.address)
        .bind(&signal.slow_swap.token_out.symbol)
        .bind(&signal.slow_swap.token_out.address)
        .bind(&signal.slow_swap.amount_in)
        .bind(&signal.slow_swap.amount_out)
        .bind(&signal.fast_swap.token_in.symbol)
        .bind(&signal.fast_swap.token_in.address)
        .bind(&signal.fast_swap.token_out.symbol)
        .bind(&signal.fast_swap.token_out.address)
        .bind(&signal.fast_swap.amount_in)
        .bind(&signal.fast_swap.amount_out)
        .bind(&signal.surplus_a)
        .bind(&signal.surplus_b)
        .bind(&signal.expected_profit_a)
        .bind(&signal.expected_profit_b)
        .bind(signal.max_slippage_bps as i64)
        .execute(&*self.pool)
        .await?;

        info!(
            "Inserted arbitrage signal for block {}",
            signal.block_height
        );
        Ok(())
    }

    #[instrument(skip(self))]
    #[allow(dead_code)]
    pub async fn get_recent(&self, limit: u32) -> Result<Vec<ArbitrageSignal>> {
        let rows = sqlx::query(
            r#"
            SELECT
                block_height, slow_chain, slow_pair_token_a_symbol, slow_pair_token_a_address,
                slow_pair_token_b_symbol, slow_pair_token_b_address, slow_pool_id,
                fast_chain, fast_pair_token_a_symbol, fast_pair_token_a_address,
                fast_pair_token_b_symbol, fast_pair_token_b_address, fast_pool_id,
                slow_swap_token_in_symbol, slow_swap_token_in_address,
                slow_swap_token_out_symbol, slow_swap_token_out_address,
                slow_swap_amount_in, slow_swap_amount_out,
                fast_swap_token_in_symbol, fast_swap_token_in_address,
                fast_swap_token_out_symbol, fast_swap_token_out_address,
                fast_swap_amount_in, fast_swap_amount_out,
                surplus_a, surplus_b, expected_profit_a, expected_profit_b, max_slippage_bps
            FROM arbitrage_signals
            ORDER BY block_height DESC
            LIMIT $1
            "#,
        )
        .bind(limit as i64)
        .fetch_all(&*self.pool)
        .await?;

        rows.into_iter()
            .map(|row| CrossChainSingleHop::try_from_row(row))
            .collect()
    }

    #[instrument(skip(self))]
    pub async fn count_by_block_height(&self, block_height: u64) -> Result<u64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) as count
            FROM arbitrage_signals
            WHERE block_height = $1
            "#,
        )
        .bind(block_height as i64)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count as u64)
    }

    pub async fn get_by_block_height(
        &self,
        block_height: u64,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ArbitrageSignal>> {
        let rows = sqlx::query(
            r#"
            SELECT
                block_height, slow_chain, slow_pair_token_a_symbol, slow_pair_token_a_address,
                slow_pair_token_b_symbol, slow_pair_token_b_address, slow_pool_id,
                fast_chain, fast_pair_token_a_symbol, fast_pair_token_a_address,
                fast_pair_token_b_symbol, fast_pair_token_b_address, fast_pool_id,
                slow_swap_token_in_symbol, slow_swap_token_in_address,
                slow_swap_token_out_symbol, slow_swap_token_out_address,
                slow_swap_amount_in, slow_swap_amount_out,
                fast_swap_token_in_symbol, fast_swap_token_in_address,
                fast_swap_token_out_symbol, fast_swap_token_out_address,
                fast_swap_amount_in, fast_swap_amount_out,
                surplus_a, surplus_b, expected_profit_a, expected_profit_b, max_slippage_bps
            FROM arbitrage_signals
            WHERE block_height = $1
            ORDER BY id
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(block_height as i64)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&*self.pool)
        .await?;

        rows.into_iter()
            .map(|row| CrossChainSingleHop::try_from_row(row))
            .collect()
    }

    #[instrument(skip(self))]
    #[allow(dead_code)]
    pub async fn get_by_block_range(
        &self,
        start_block: u64,
        end_block: u64,
    ) -> Result<Vec<ArbitrageSignal>> {
        let rows = sqlx::query(
            r#"
            SELECT
                block_height, slow_chain, slow_pair_token_a_symbol, slow_pair_token_a_address,
                slow_pair_token_b_symbol, slow_pair_token_b_address, slow_pool_id,
                fast_chain, fast_pair_token_a_symbol, fast_pair_token_a_address,
                fast_pair_token_b_symbol, fast_pair_token_b_address, fast_pool_id,
                slow_swap_token_in_symbol, slow_swap_token_in_address,
                slow_swap_token_out_symbol, slow_swap_token_out_address,
                slow_swap_amount_in, slow_swap_amount_out,
                fast_swap_token_in_symbol, fast_swap_token_in_address,
                fast_swap_token_out_symbol, fast_swap_token_out_address,
                fast_swap_amount_in, fast_swap_amount_out,
                surplus_a, surplus_b, expected_profit_a, expected_profit_b, max_slippage_bps
            FROM arbitrage_signals
            WHERE block_height BETWEEN $1 AND $2
            ORDER BY block_height ASC
            "#,
        )
        .bind(start_block as i64)
        .bind(end_block as i64)
        .fetch_all(&*self.pool)
        .await?;

        rows.into_iter()
            .map(|row| CrossChainSingleHop::try_from_row(row))
            .collect()
    }

    #[instrument(skip(self))]
    pub async fn get_by_chain(
        &self,
        chain: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ArbitrageSignal>> {
        let rows = sqlx::query(
            r#"
            SELECT
                block_height, slow_chain, slow_pair_token_a_symbol, slow_pair_token_a_address,
                slow_pair_token_b_symbol, slow_pair_token_b_address, slow_pool_id,
                fast_chain, fast_pair_token_a_symbol, fast_pair_token_a_address,
                fast_pair_token_b_symbol, fast_pair_token_b_address, fast_pool_id,
                slow_swap_token_in_symbol, slow_swap_token_in_address,
                slow_swap_token_out_symbol, slow_swap_token_out_address,
                slow_swap_amount_in, slow_swap_amount_out,
                fast_swap_token_in_symbol, fast_swap_token_in_address,
                fast_swap_token_out_symbol, fast_swap_token_out_address,
                fast_swap_amount_in, fast_swap_amount_out,
                surplus_a, surplus_b, expected_profit_a, expected_profit_b, max_slippage_bps
            FROM arbitrage_signals
            WHERE slow_chain = $1 OR fast_chain = $1
            ORDER BY block_height DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(chain)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&*self.pool)
        .await?;

        rows.into_iter()
            .map(|row| CrossChainSingleHop::try_from_row(row))
            .collect()
    }

    #[instrument(skip(self))]
    pub async fn count_by_chain(&self, chain: &str) -> Result<u64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) as count
            FROM arbitrage_signals
            WHERE slow_chain = $1 OR fast_chain = $1
            "#,
        )
        .bind(chain)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count as u64)
    }

    #[instrument(skip(self))]
    pub async fn get_by_pair(
        &self,
        pair: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ArbitrageSignal>> {
        let parts: Vec<&str> = pair.split('-').collect();
        if parts.len() != 2 {
            return Ok(vec![]);
        }
        let token_a = parts[0];
        let token_b = parts[1];

        let rows = sqlx::query(
            r#"
            SELECT
                block_height, slow_chain, slow_pair_token_a_symbol, slow_pair_token_a_address,
                slow_pair_token_b_symbol, slow_pair_token_b_address, slow_pool_id,
                fast_chain, fast_pair_token_a_symbol, fast_pair_token_a_address,
                fast_pair_token_b_symbol, fast_pair_token_b_address, fast_pool_id,
                slow_swap_token_in_symbol, slow_swap_token_in_address,
                slow_swap_token_out_symbol, slow_swap_token_out_address,
                slow_swap_amount_in, slow_swap_amount_out,
                fast_swap_token_in_symbol, fast_swap_token_in_address,
                fast_swap_token_out_symbol, fast_swap_token_out_address,
                fast_swap_amount_in, fast_swap_amount_out,
                surplus_a, surplus_b, expected_profit_a, expected_profit_b, max_slippage_bps
            FROM arbitrage_signals
            WHERE (
                (slow_pair_token_a_symbol = $1 AND slow_pair_token_b_symbol = $2) OR
                (slow_pair_token_a_symbol = $2 AND slow_pair_token_b_symbol = $1) OR
                (fast_pair_token_a_symbol = $1 AND fast_pair_token_b_symbol = $2) OR
                (fast_pair_token_a_symbol = $2 AND fast_pair_token_b_symbol = $1)
            )
            ORDER BY block_height DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(token_a)
        .bind(token_b)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&*self.pool)
        .await?;

        rows.into_iter()
            .map(|row| CrossChainSingleHop::try_from_row(row))
            .collect()
    }

    #[instrument(skip(self))]
    pub async fn count_by_pair(&self, pair: &str) -> Result<u64> {
        let parts: Vec<&str> = pair.split('-').collect();
        if parts.len() != 2 {
            return Ok(0);
        }
        let token_a = parts[0];
        let token_b = parts[1];

        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) as count
            FROM arbitrage_signals
            WHERE (
                (slow_pair_token_a_symbol = $1 AND slow_pair_token_b_symbol = $2) OR
                (slow_pair_token_a_symbol = $2 AND slow_pair_token_b_symbol = $1) OR
                (fast_pair_token_a_symbol = $1 AND fast_pair_token_b_symbol = $2) OR
                (fast_pair_token_a_symbol = $2 AND fast_pair_token_b_symbol = $1)
            )
            "#,
        )
        .bind(token_a)
        .bind(token_b)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count as u64)
    }
}
