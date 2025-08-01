use std::sync::Arc;

use color_eyre::eyre;
use sqlx::PgPool;
use tracing::instrument;

use crate::{config::TokenAddressesForChain, signals::CrossChainSingleHop};

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

    // #[instrument(skip(self, signal))]
    // #[allow(dead_code)]
    // pub async fn insert(&self, signal: &CrossChainSingleHop) -> eyre::Result<()> {
    //     sqlx::query(
    //         r#"
    //         INSERT INTO arbitrage_signals (
    //             block_height, slow_chain, slow_pair_token_a_symbol, slow_pair_token_a_address,
    //             slow_pair_token_b_symbol, slow_pair_token_b_address, slow_pool_id,
    //             fast_chain, fast_pair_token_a_symbol, fast_pair_token_a_address,
    //             fast_pair_token_b_symbol, fast_pair_token_b_address, fast_pool_id,
    //             slow_swap_token_in_symbol, slow_swap_token_in_address,
    //             slow_swap_token_out_symbol, slow_swap_token_out_address,
    //             slow_swap_amount_in, slow_swap_amount_out,
    //             fast_swap_token_in_symbol, fast_swap_token_in_address,
    //             fast_swap_token_out_symbol, fast_swap_token_out_address,
    //             fast_swap_amount_in, fast_swap_amount_out,
    //             surplus_a, surplus_b, expected_profit_a, expected_profit_b, max_slippage_bps
    //         ) VALUES (
    //             $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
    //             $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25,
    //             $26, $27, $28, $29, $30
    //         )
    //         "#,
    //     )
    //     .bind(signal.block_height as i64)
    //     .bind(&signal.slow_chain)
    //     .bind(&signal.slow_pair.token_a.symbol)
    //     .bind(&signal.slow_pair.token_a.address)
    //     .bind(&signal.slow_pair.token_b.symbol)
    //     .bind(&signal.slow_pair.token_b.address)
    //     .bind(&signal.slow_pool_id)
    //     .bind(&signal.fast_chain)
    //     .bind(&signal.fast_pair.token_a.symbol)
    //     .bind(&signal.fast_pair.token_a.address)
    //     .bind(&signal.fast_pair.token_b.symbol)
    //     .bind(&signal.fast_pair.token_b.address)
    //     .bind(&signal.fast_pool_id)
    //     .bind(&signal.slow_swap.token_in.symbol)
    //     .bind(&signal.slow_swap.token_in.address)
    //     .bind(&signal.slow_swap.token_out.symbol)
    //     .bind(&signal.slow_swap.token_out.address)
    //     .bind(&signal.slow_swap.amount_in)
    //     .bind(&signal.slow_swap.amount_out)
    //     .bind(&signal.fast_swap.token_in.symbol)
    //     .bind(&signal.fast_swap.token_in.address)
    //     .bind(&signal.fast_swap.token_out.symbol)
    //     .bind(&signal.fast_swap.token_out.address)
    //     .bind(&signal.fast_swap.amount_in)
    //     .bind(&signal.fast_swap.amount_out)
    //     .bind(&signal.surplus_a)
    //     .bind(&signal.surplus_b)
    //     .bind(&signal.expected_profit_a)
    //     .bind(&signal.expected_profit_b)
    //     .bind(signal.max_slippage_bps as i64)
    //     .execute(&*self.pool)
    //     .await?;

    //     info!(
    //         "Inserted arbitrage signal for block {}",
    //         signal.block_height
    //     );
    //     Ok(())
    // }

    #[instrument(skip(self))]
    #[allow(dead_code)]
    pub async fn get_recent(&self, limit: u32) -> eyre::Result<Vec<CrossChainSingleHop>> {
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
    pub async fn count_by_block_height(&self, block_height: u64) -> eyre::Result<u64> {
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
    ) -> eyre::Result<Vec<CrossChainSingleHop>> {
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
    ) -> eyre::Result<Vec<CrossChainSingleHop>> {
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
    ) -> eyre::Result<Vec<CrossChainSingleHop>> {
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
    pub async fn count_by_chain(&self, chain: &str) -> eyre::Result<u64> {
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
    ) -> eyre::Result<Vec<CrossChainSingleHop>> {
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
    pub async fn count_by_pair(&self, pair: &str) -> eyre::Result<u64> {
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
