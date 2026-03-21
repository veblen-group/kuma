use std::sync::Arc;

use color_eyre::eyre::{self};
use sqlx::PgPool;
use tracing::instrument;

use crate::{
    config::TokenAddressesForChain,
    trade::{
        TradeFailedOnFast, TradeFailedOnFastInsertRow, TradeFailedOnSlow, TradeFailedOnSlowInsertRow,
        TradeResult, TradeSuccess, TradeSuccessInsertRow,
    },
};

/// Full signal data joined into a successful trade row — avoids N+1 lookups in the backend.
pub struct TradeSuccessRow {
    pub signal_id: i64,
    pub slow_tx_hash: String,
    pub fast_tx_hash: String,
    pub realized_profit_str: String,
    // Joined signal fields
    pub slow_chain: String,
    pub slow_height: i64,
    pub slow_pool_id: String,
    pub fast_chain: String,
    pub fast_height: i64,
    pub fast_pool_id: String,
    pub slow_swap_token_in_symbol: String,
    pub slow_swap_token_out_symbol: String,
    pub slow_swap_amount_in: String,
    pub slow_swap_amount_out: String,
    pub slow_swap_gas_cost: String,
    pub fast_swap_token_in_symbol: String,
    pub fast_swap_token_out_symbol: String,
    pub fast_swap_amount_in: String,
    pub fast_swap_amount_out: String,
    pub fast_swap_gas_cost: String,
    pub surplus_a: String,
    pub surplus_b: String,
    pub min_token_amount_a: String,
    pub min_token_amount_b: String,
    pub min_usdc_amount_a: String,
    pub min_usdc_amount_b: String,
    pub min_total_amount_usdc: String,
    pub max_slippage_token_amount_a: String,
    pub max_slippage_token_amount_b: String,
    pub token_usdc_price_a: f64,
    pub token_usdc_price_b: f64,
    pub gas_cost_eth_slow: String,
    pub gas_cost_eth_fast: String,
    pub total_gas_cost_eth: String,
    pub eth_usdc_price: f64,
    pub gas_cost_usdc_slow: String,
    pub gas_cost_usdc_fast: String,
    pub total_gas_cost_usdc: String,
    pub slow_base_fee: i64,
    pub fast_base_fee: i64,
    pub max_slippage_bps: i64,
    pub congestion_risk_discount_bps: i64,
}

/// Full signal data joined into a failed-on-slow trade row.
pub struct TradeFailedOnSlowRow {
    pub signal_id: i64,
    pub slow_tx_hash: Option<String>,
    // Joined signal fields
    pub slow_chain: String,
    pub slow_height: i64,
    pub slow_pool_id: String,
    pub fast_chain: String,
    pub fast_height: i64,
    pub fast_pool_id: String,
    pub slow_swap_token_in_symbol: String,
    pub slow_swap_token_out_symbol: String,
    pub slow_swap_amount_in: String,
    pub slow_swap_amount_out: String,
    pub slow_swap_gas_cost: String,
    pub fast_swap_token_in_symbol: String,
    pub fast_swap_token_out_symbol: String,
    pub fast_swap_amount_in: String,
    pub fast_swap_amount_out: String,
    pub fast_swap_gas_cost: String,
    pub surplus_a: String,
    pub surplus_b: String,
    pub min_token_amount_a: String,
    pub min_token_amount_b: String,
    pub min_usdc_amount_a: String,
    pub min_usdc_amount_b: String,
    pub min_total_amount_usdc: String,
    pub max_slippage_token_amount_a: String,
    pub max_slippage_token_amount_b: String,
    pub token_usdc_price_a: f64,
    pub token_usdc_price_b: f64,
    pub gas_cost_eth_slow: String,
    pub gas_cost_eth_fast: String,
    pub total_gas_cost_eth: String,
    pub eth_usdc_price: f64,
    pub gas_cost_usdc_slow: String,
    pub gas_cost_usdc_fast: String,
    pub total_gas_cost_usdc: String,
    pub slow_base_fee: i64,
    pub fast_base_fee: i64,
    pub max_slippage_bps: i64,
    pub congestion_risk_discount_bps: i64,
}

/// Full signal data joined into a failed-on-fast trade row.
pub struct TradeFailedOnFastRow {
    pub signal_id: i64,
    pub slow_tx_hash: String,
    pub fast_tx_hash: Option<String>,
    // Joined signal fields
    pub slow_chain: String,
    pub slow_height: i64,
    pub slow_pool_id: String,
    pub fast_chain: String,
    pub fast_height: i64,
    pub fast_pool_id: String,
    pub slow_swap_token_in_symbol: String,
    pub slow_swap_token_out_symbol: String,
    pub slow_swap_amount_in: String,
    pub slow_swap_amount_out: String,
    pub slow_swap_gas_cost: String,
    pub fast_swap_token_in_symbol: String,
    pub fast_swap_token_out_symbol: String,
    pub fast_swap_amount_in: String,
    pub fast_swap_amount_out: String,
    pub fast_swap_gas_cost: String,
    pub surplus_a: String,
    pub surplus_b: String,
    pub min_token_amount_a: String,
    pub min_token_amount_b: String,
    pub min_usdc_amount_a: String,
    pub min_usdc_amount_b: String,
    pub min_total_amount_usdc: String,
    pub max_slippage_token_amount_a: String,
    pub max_slippage_token_amount_b: String,
    pub token_usdc_price_a: f64,
    pub token_usdc_price_b: f64,
    pub gas_cost_eth_slow: String,
    pub gas_cost_eth_fast: String,
    pub total_gas_cost_eth: String,
    pub eth_usdc_price: f64,
    pub gas_cost_usdc_slow: String,
    pub gas_cost_usdc_fast: String,
    pub total_gas_cost_usdc: String,
    pub slow_base_fee: i64,
    pub fast_base_fee: i64,
    pub max_slippage_bps: i64,
    pub congestion_risk_discount_bps: i64,
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
        let insert = trade.into_row();
        self.insert_successful(insert).await
    }

    async fn insert_successful(&self, insert: TradeSuccessInsertRow) -> eyre::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO successful_trade (
                signal_id, slow_tx_hash, fast_tx_hash, realized_profit_str
            ) VALUES (
                $1, $2, $3, $4
            )
            "#,
            insert.signal_id,
            insert.slow_tx_hash,
            insert.fast_tx_hash,
            insert.realized_profit_str
        )
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    #[instrument(skip(self, trade_result))]
    pub async fn insert_failed_on_slow(&self, trade_result: TradeFailedOnSlow) -> eyre::Result<()> {
        let insert = trade_result.into_row();
        self.insert_failed_slow(insert).await
    }

    async fn insert_failed_slow(&self, insert: TradeFailedOnSlowInsertRow) -> eyre::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO failed_on_slow_chain_trade (
                signal_id, slow_tx_hash
            ) VALUES (
                $1, $2
            )
            "#,
            insert.signal_id,
            insert.slow_tx_hash,
        )
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    #[instrument(skip(self, trade_result))]
    pub async fn insert_failed_on_fast(&self, trade_result: TradeFailedOnFast) -> eyre::Result<()> {
        let insert = trade_result.into_row();
        self.insert_failed_fast(insert).await
    }

    async fn insert_failed_fast(&self, insert: TradeFailedOnFastInsertRow) -> eyre::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO failed_on_fast_chain_trade (
                signal_id, slow_tx_hash, fast_tx_hash
            ) VALUES (
                $1, $2, $3
            )
            "#,
            insert.signal_id,
            insert.slow_tx_hash,
            insert.fast_tx_hash,
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
                st.signal_id, st.slow_tx_hash, st.fast_tx_hash, st.realized_profit_str,
                s.slow_chain, s.slow_height, s.slow_pool_id,
                s.fast_chain, s.fast_height, s.fast_pool_id,
                s.slow_swap_token_in_symbol, s.slow_swap_token_out_symbol,
                s.slow_swap_amount_in, s.slow_swap_amount_out, s.slow_swap_gas_cost,
                s.fast_swap_token_in_symbol, s.fast_swap_token_out_symbol,
                s.fast_swap_amount_in, s.fast_swap_amount_out, s.fast_swap_gas_cost,
                s.surplus_a, s.surplus_b,
                s.min_token_amount_a, s.min_token_amount_b,
                s.min_usdc_amount_a, s.min_usdc_amount_b, s.min_total_amount_usdc,
                s.max_slippage_token_amount_a, s.max_slippage_token_amount_b,
                s.token_usdc_price_a, s.token_usdc_price_b,
                s.gas_cost_eth_slow, s.gas_cost_eth_fast, s.total_gas_cost_eth,
                s.eth_usdc_price,
                s.gas_cost_usdc_slow, s.gas_cost_usdc_fast, s.total_gas_cost_usdc,
                s.slow_base_fee, s.fast_base_fee,
                s.max_slippage_bps, s.congestion_risk_discount_bps
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
                fst.signal_id, fst.slow_tx_hash,
                s.slow_chain, s.slow_height, s.slow_pool_id,
                s.fast_chain, s.fast_height, s.fast_pool_id,
                s.slow_swap_token_in_symbol, s.slow_swap_token_out_symbol,
                s.slow_swap_amount_in, s.slow_swap_amount_out, s.slow_swap_gas_cost,
                s.fast_swap_token_in_symbol, s.fast_swap_token_out_symbol,
                s.fast_swap_amount_in, s.fast_swap_amount_out, s.fast_swap_gas_cost,
                s.surplus_a, s.surplus_b,
                s.min_token_amount_a, s.min_token_amount_b,
                s.min_usdc_amount_a, s.min_usdc_amount_b, s.min_total_amount_usdc,
                s.max_slippage_token_amount_a, s.max_slippage_token_amount_b,
                s.token_usdc_price_a, s.token_usdc_price_b,
                s.gas_cost_eth_slow, s.gas_cost_eth_fast, s.total_gas_cost_eth,
                s.eth_usdc_price,
                s.gas_cost_usdc_slow, s.gas_cost_usdc_fast, s.total_gas_cost_usdc,
                s.slow_base_fee, s.fast_base_fee,
                s.max_slippage_bps, s.congestion_risk_discount_bps
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
                fft.signal_id, fft.slow_tx_hash, fft.fast_tx_hash,
                s.slow_chain, s.slow_height, s.slow_pool_id,
                s.fast_chain, s.fast_height, s.fast_pool_id,
                s.slow_swap_token_in_symbol, s.slow_swap_token_out_symbol,
                s.slow_swap_amount_in, s.slow_swap_amount_out, s.slow_swap_gas_cost,
                s.fast_swap_token_in_symbol, s.fast_swap_token_out_symbol,
                s.fast_swap_amount_in, s.fast_swap_amount_out, s.fast_swap_gas_cost,
                s.surplus_a, s.surplus_b,
                s.min_token_amount_a, s.min_token_amount_b,
                s.min_usdc_amount_a, s.min_usdc_amount_b, s.min_total_amount_usdc,
                s.max_slippage_token_amount_a, s.max_slippage_token_amount_b,
                s.token_usdc_price_a, s.token_usdc_price_b,
                s.gas_cost_eth_slow, s.gas_cost_eth_fast, s.total_gas_cost_eth,
                s.eth_usdc_price,
                s.gas_cost_usdc_slow, s.gas_cost_usdc_fast, s.total_gas_cost_usdc,
                s.slow_base_fee, s.fast_base_fee,
                s.max_slippage_bps, s.congestion_risk_discount_bps
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
