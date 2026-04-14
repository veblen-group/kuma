//! Trade result persistence.
//!
//! `TradeRepository` writes the three possible trade outcomes to separate tables:
//!
//! | Outcome | Table |
//! |---------|-------|
//! | `TradeResult::FailedSlow` | `failed_on_slow_chain_trade` |
//! | `TradeResult::FailedFast` | `failed_on_fast_chain_trade` |
//! | `TradeResult::Successful` | `successful_trade` |
//!
//! ## Signal FK lookup
//!
//! Trade inserts do not receive the signal's database ID directly. Instead they use a
//! subquery to look it up at insert time by matching on
//! `(slow_chain, slow_height, slow_pool_id, fast_chain, fast_height, fast_pool_id)`:
//!
//! ```sql
//! INSERT INTO successful_trade (signal_id, ...)
//! SELECT id, ... FROM signals
//! WHERE slow_chain = $1 AND slow_height = $2 AND slow_pool_id = $3
//!   AND fast_chain = $4 AND fast_height = $5 AND fast_pool_id = $6
//! ```
//!
//! This avoids threading the signal DB ID through the entire execution pipeline.

use std::sync::Arc;

use color_eyre::eyre::{self};
use sqlx::PgPool;
use tracing::instrument;

use crate::{
    config::TokenAddressesForChain,
    trade::{TradeFailedOnFast, TradeFailedOnSlow, TradeResult, TradeSuccess},
};

pub struct TradeFailedOnSlowRow {
    pub id: i64,
    pub signal_id: i64,
    pub slow_tx_hash: Option<String>,
}

pub struct TradeFailedOnFastRow {
    pub id: i64,
    pub signal_id: i64,
    pub slow_tx_hash: String,
    pub fast_tx_hash: Option<String>,
}

pub struct TradeSuccessRow {
    pub id: i64,
    pub signal_id: i64,
    pub slow_tx_hash: String,
    pub fast_tx_hash: String,
    pub realized_profit_str: String,
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

    /// Dispatch to the appropriate insert method based on trade outcome.
    pub async fn insert_trade_result(&self, trade_result: TradeResult) -> eyre::Result<()> {
        match trade_result {
            TradeResult::FailedSlow(t) => self.insert_failed_on_slow(t).await,
            TradeResult::FailedFast(t) => self.insert_failed_on_fast(t).await,
            TradeResult::Successful(t) => self.insert_successful_trade(t).await,
        }
    }

    /// Record a trade where the slow-chain leg failed or was not included.
    /// The fast-chain leg was never submitted. `slow_tx_hash` is `None` if the tx
    /// was not included (timed out before the next block).
    #[instrument(skip(self, trade))]
    pub async fn insert_failed_on_slow(&self, trade: TradeFailedOnSlow) -> eyre::Result<()> {
        let slow_hash = trade.slow_receipt.map(|r| r.transaction_hash.to_string());

        sqlx::query!(
            r#"
                INSERT INTO failed_on_slow_chain_trade (signal_id, slow_tx_hash)
                SELECT id, $7 FROM signals
                WHERE slow_chain = $1 AND slow_height = $2 AND slow_pool_id = $3
                  AND fast_chain = $4 AND fast_height = $5 AND fast_pool_id = $6
                "#,
            trade.signal.slow_chain.name.to_string(), // $1
            trade.signal.slow_height as i64,          // $2
            trade.signal.slow_pool_id.to_string(),    // $3
            trade.signal.fast_chain.name.to_string(), // $4
            trade.signal.fast_height as i64,          // $5
            trade.signal.fast_pool_id.to_string(),    // $6
            slow_hash                                 // $7
        )
        .execute(self.pool.as_ref())
        .await?;

        Ok(())
    }

    /// Record a trade where the slow-chain leg succeeded but the fast-chain leg failed.
    /// This is the most costly failure — the slow-chain position must be unwound.
    #[instrument(skip(self, trade))]
    pub async fn insert_failed_on_fast(&self, trade: TradeFailedOnFast) -> eyre::Result<()> {
        let slow_hash = trade.slow_receipt.transaction_hash.to_string();
        let fast_hash = trade.fast_receipt.map(|r| r.transaction_hash.to_string());

        sqlx::query!(
            r#"
            INSERT INTO failed_on_fast_chain_trade (signal_id, slow_tx_hash, fast_tx_hash)
            SELECT id, $7, $8 FROM signals
            WHERE slow_chain = $1 AND slow_height = $2 AND slow_pool_id = $3
              AND fast_chain = $4 AND fast_height = $5 AND fast_pool_id = $6
            "#,
            trade.signal.slow_chain.name.to_string(), // $1
            trade.signal.slow_height as i64,          // $2
            trade.signal.slow_pool_id.to_string(),    // $3
            trade.signal.fast_chain.name.to_string(), // $4
            trade.signal.fast_height as i64,          // $5
            trade.signal.fast_pool_id.to_string(),    // $6
            slow_hash,                                // $7
            fast_hash                                 // $8
        )
        .execute(self.pool.as_ref())
        .await?;

        Ok(())
    }

    pub async fn insert_successful_trade(&self, trade: TradeSuccess) -> eyre::Result<()> {
        let slow_hash = trade.slow_receipt.transaction_hash.to_string();
        let fast_hash = trade.fast_receipt.transaction_hash.to_string();
        let profit = trade.realized_profit.total_usdc.to_string();

        sqlx::query!(
            r#"
            INSERT INTO successful_trade (signal_id, slow_tx_hash, fast_tx_hash, realized_profit_str)
            SELECT id, $7, $8, $9 FROM signals
            WHERE slow_chain = $1 AND slow_height = $2 AND slow_pool_id = $3
              AND fast_chain = $4 AND fast_height = $5 AND fast_pool_id = $6
            "#,
            trade.signal.slow_chain.name.to_string(), // $1
            trade.signal.slow_height as i64,          // $2
            trade.signal.slow_pool_id.to_string(),    // $3
            trade.signal.fast_chain.name.to_string(), // $4
            trade.signal.fast_height as i64,          // $5
            trade.signal.fast_pool_id.to_string(),    // $6
            slow_hash,                                // $7
            fast_hash,                                // $8
            profit                                    // $9
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
            SELECT COUNT(*) FROM successful_trade st
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
            SELECT COUNT(*) FROM failed_on_slow_chain_trade fst
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
            SELECT COUNT(*) FROM failed_on_fast_chain_trade fft
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
            SELECT st.id, st.signal_id, st.slow_tx_hash, st.fast_tx_hash, st.realized_profit_str
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
            offset as i64,
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
            SELECT fst.id, fst.signal_id, fst.slow_tx_hash
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
            offset as i64,
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
            SELECT fft.id, fft.signal_id, fft.slow_tx_hash, fft.fast_tx_hash
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
            offset as i64,
        )
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }
}
