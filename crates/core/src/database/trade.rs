use std::sync::Arc;

use color_eyre::eyre::{self};
use sqlx::PgPool;
use tracing::instrument;

use crate::{
    config::TokenAddressesForChain,
    trade::{TradeFailedOnFast, TradeFailedOnSlow, TradeResult, TradeSuccess},
};

pub struct TradeSuccessRow {
    pub signal_id: i64,
    pub slow_tx_hash: String,
    pub fast_tx_hash: String,
    pub realized_profit_str: String,
}

pub struct TradeFailedOnSlowRow {
    pub signal_id: i64,
    pub slow_tx_hash: Option<String>,
}

pub struct TradeFailedOnFastRow {
    pub signal_id: i64,
    pub slow_tx_hash: String,
    pub fast_tx_hash: Option<String>,
}

#[derive(Clone)]
pub struct TradeRepository {
    pool: Arc<PgPool>,
    #[allow(unused)]
    tokens_config: Arc<TokenAddressesForChain>,
}

impl TradeRepository {
    pub(super) fn new(pool: Arc<PgPool>, tokens_config: Arc<TokenAddressesForChain>) -> Self {
        Self {
            pool,
            tokens_config,
        }
    }

    // #[instrument(skip(self, trade_result))]
    pub async fn insert_trade_result(&self, trade_result: TradeResult) -> eyre::Result<()> {
        match trade_result {
            TradeResult::Successful(trade_success) => {
                self.insert_successful_trade(trade_success).await
            }
            TradeResult::FailedSlow(trade_failed_on_slow) => {
                self.insert_failed_on_slow(trade_failed_on_slow).await
            }
            TradeResult::FailedFast(trade_failed_on_fast) => {
                self.insert_failed_on_fast(trade_failed_on_fast).await
            }
        }
    }

    pub async fn insert_successful_trade(&self, trade: TradeSuccess) -> eyre::Result<()> {
        let row = trade.into_row();

        sqlx::query!(
            r#"
            INSERT INTO successful_trade (
                signal_id, slow_tx_hash, fast_tx_hash, realized_profit_str
            ) VALUES (
                $1, $2, $3, $4
            )
            "#,
            row.signal_id as i64,
            row.slow_tx_hash,
            row.fast_tx_hash,
            row.realized_profit_str
        )
        .execute(self.pool.as_ref())
        .await?;

        Ok(())
    }

    #[instrument(skip(self, trade_result))]
    pub async fn insert_failed_on_slow(&self, trade_result: TradeFailedOnSlow) -> eyre::Result<()> {
        let row = trade_result.into_row();

        sqlx::query!(
            r#"
            INSERT INTO failed_on_slow_chain_trade (
                signal_id, slow_tx_hash
            ) VALUES (
                $1, $2
            )
            "#,
            row.signal_id as i64,
            row.slow_tx_hash,
        )
        .execute(self.pool.as_ref())
        .await?;

        Ok(())
    }

    #[instrument(skip(self, trade_result))]
    pub async fn insert_failed_on_fast(&self, trade_result: TradeFailedOnFast) -> eyre::Result<()> {
        let row = trade_result.into_row();

        sqlx::query!(
            r#"
            INSERT INTO failed_on_fast_chain_trade (
                signal_id, slow_tx_hash, fast_tx_hash
            ) VALUES (
                $1, $2, $3
            )
            "#,
            row.signal_id as i64,
            row.slow_tx_hash,
            row.fast_tx_hash,
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
        let success = self
            .count_successful_by_symbols(token_a_symbol, token_b_symbol)
            .await?;
        let failed_on_slow = self
            .count_failed_on_slow_by_symbols(token_a_symbol, token_b_symbol)
            .await?;
        let failed_on_fast = self
            .count_failed_on_fast_by_symbols(token_a_symbol, token_b_symbol)
            .await?;

        Ok(success + failed_on_slow + failed_on_fast)
    }

    #[instrument(skip(self))]
    pub async fn count_successful_by_symbols(
        &self,
        token_a_symbol: &str,
        token_b_symbol: &str,
    ) -> eyre::Result<u64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM successful_trade st
            JOIN signals s ON st.signal_id = s.id
            WHERE (
                (s.slow_swap_token_in_symbol = $1 AND s.slow_swap_token_out_symbol = $2)
                OR (s.slow_swap_token_in_symbol = $2 AND s.slow_swap_token_out_symbol = $1)
            )
            "#,
        )
        .bind(token_a_symbol)
        .bind(token_b_symbol)
        .fetch_one(self.pool.as_ref())
        .await?;

        Ok(count as u64)
    }

    #[instrument(skip(self))]
    pub async fn count_failed_on_slow_by_symbols(
        &self,
        token_a_symbol: &str,
        token_b_symbol: &str,
    ) -> eyre::Result<u64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM failed_on_slow_chain_trade fst
            JOIN signals s ON fst.signal_id = s.id
            WHERE (
                (s.slow_swap_token_in_symbol = $1 AND s.slow_swap_token_out_symbol = $2)
                OR (s.slow_swap_token_in_symbol = $2 AND s.slow_swap_token_out_symbol = $1)
            )
            "#,
        )
        .bind(token_a_symbol)
        .bind(token_b_symbol)
        .fetch_one(self.pool.as_ref())
        .await?;

        Ok(count as u64)
    }

    #[instrument(skip(self))]
    pub async fn count_failed_on_fast_by_symbols(
        &self,
        token_a_symbol: &str,
        token_b_symbol: &str,
    ) -> eyre::Result<u64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM failed_on_fast_chain_trade fft
            JOIN signals s ON fft.signal_id = s.id
            WHERE (
                (s.slow_swap_token_in_symbol = $1 AND s.slow_swap_token_out_symbol = $2)
                OR (s.slow_swap_token_in_symbol = $2 AND s.slow_swap_token_out_symbol = $1)
            )
            "#,
        )
        .bind(token_a_symbol)
        .bind(token_b_symbol)
        .fetch_one(self.pool.as_ref())
        .await?;

        Ok(count as u64)
    }

    pub async fn get_successful_by_symbols(
        &self,
        token_a_symbol: &str,
        token_b_symbol: &str,
        limit: u32,
        offset: u32,
    ) -> eyre::Result<Vec<TradeSuccessRow>> {
        let rows = sqlx::query_as!(
            TradeSuccessRow,
            r#"
            SELECT
                st.signal_id,
                st.slow_tx_hash,
                st.fast_tx_hash,
                st.realized_profit_str
            FROM successful_trade st
            JOIN signals s ON st.signal_id = s.id
            WHERE (
                (s.slow_swap_token_in_symbol = $1 AND s.slow_swap_token_out_symbol = $2)
                OR (s.slow_swap_token_in_symbol = $2 AND s.slow_swap_token_out_symbol = $1)
            )
            ORDER BY st.created_at DESC
            LIMIT $3 OFFSET $4
            "#,
            token_a_symbol,
            token_b_symbol,
            limit as i64,
            offset as i64
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_failed_on_slow_by_symbols(
        &self,
        token_a_symbol: &str,
        token_b_symbol: &str,
        limit: u32,
        offset: u32,
    ) -> eyre::Result<Vec<TradeFailedOnSlowRow>> {
        let rows = sqlx::query_as!(
            TradeFailedOnSlowRow,
            r#"
            SELECT
                fst.signal_id,
                fst.slow_tx_hash
            FROM failed_on_slow_chain_trade fst
            JOIN signals s ON fst.signal_id = s.id
            WHERE (
                (s.slow_swap_token_in_symbol = $1 AND s.slow_swap_token_out_symbol = $2)
                OR (s.slow_swap_token_in_symbol = $2 AND s.slow_swap_token_out_symbol = $1)
            )
            ORDER BY fst.created_at DESC
            LIMIT $3 OFFSET $4
            "#,
            token_a_symbol,
            token_b_symbol,
            limit as i64,
            offset as i64
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_failed_on_fast_by_symbols(
        &self,
        token_a_symbol: &str,
        token_b_symbol: &str,
        limit: u32,
        offset: u32,
    ) -> eyre::Result<Vec<TradeFailedOnFastRow>> {
        let rows = sqlx::query_as!(
            TradeFailedOnFastRow,
            r#"
            SELECT
                fft.signal_id,
                fft.slow_tx_hash,
                fft.fast_tx_hash
            FROM failed_on_fast_chain_trade fft
            JOIN signals s ON fft.signal_id = s.id
            WHERE (
                (s.slow_swap_token_in_symbol = $1 AND s.slow_swap_token_out_symbol = $2)
                OR (s.slow_swap_token_in_symbol = $2 AND s.slow_swap_token_out_symbol = $1)
            )
            ORDER BY fft.created_at DESC
            LIMIT $3 OFFSET $4
            "#,
            token_a_symbol,
            token_b_symbol,
            limit as i64,
            offset as i64
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }
}
