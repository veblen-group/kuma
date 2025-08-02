use std::sync::Arc;

use color_eyre::eyre;
use sqlx::PgPool;
use tracing::instrument;

use crate::{config::TokenAddressesForChain, signals};

#[derive(Clone)]
pub struct SignalRepository {
    pool: Arc<PgPool>,
    tokens_config: Arc<TokenAddressesForChain>,
}

impl SignalRepository {
    pub(super) fn new(pool: Arc<PgPool>, tokens_config: Arc<TokenAddressesForChain>) -> Self {
        Self {
            pool,
            tokens_config,
        }
    }

    #[instrument(skip(self, signal))]
    #[allow(dead_code)]
    pub async fn insert(&self, signal: &signals::CrossChainSingleHop) -> eyre::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO signals (
                slow_chain, slow_height, slow_pool_id,
                fast_chain, fast_height, fast_pool_id,
                slow_swap_token_in_symbol, slow_swap_token_out_symbol,
                slow_swap_amount_in, slow_swap_amount_out,
                fast_swap_token_in_symbol, fast_swap_token_out_symbol,
                fast_swap_amount_in, fast_swap_amount_out,
                surplus_a, surplus_b, expected_profit_a, expected_profit_b,
                max_slippage_bps, congestion_risk_discount_bps
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                $14, $15, $16, $17, $18, $19, $20
            )
            "#,
            &signal.slow_chain.name.to_string(),
            signal.slow_height as i64,
            &signal.slow_pool_id.to_string(),
            &signal.fast_chain.name.to_string(),
            signal.fast_height as i64,
            &signal.fast_pool_id.to_string(),
            &signal.slow_swap_sim.token_in.symbol,
            &signal.slow_swap_sim.token_out.symbol,
            &signal.slow_swap_sim.amount_in.to_string(),
            &signal.slow_swap_sim.amount_out.to_string(),
            &signal.fast_swap_sim.token_in.symbol,
            &signal.fast_swap_sim.token_out.symbol,
            &signal.fast_swap_sim.amount_in.to_string(),
            &signal.fast_swap_sim.amount_out.to_string(),
            &signal.surplus.0.to_string(),
            &signal.surplus.1.to_string(),
            &signal.expected_profit.0.to_string(),
            &signal.expected_profit.1.to_string(),
            signal.max_slippage_bps as i64,
            signal.congestion_risk_discount_bps as i64,
        )
        .execute(self.pool.as_ref())
        .await?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn count_by_symbols(
        &self,
        token_a_symbol: &str,
        token_b_symbol: &str,
    ) -> eyre::Result<u64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) as count
            FROM signals
            WHERE block_height = $1
            WHERE (((slow_swap_token_in_symbol = $1 AND slow_swap_token_out_symbol = $2)
                AND (fast_swap_token_in_symbol = $2 AND fast_swap_token_out_symbol = $1))
                OR ((fast_swap_token_in_symbol = $1 AND fast_swap_token_out_symbol = $2)
                AND (fast_swap_token_in_symbol = $1 AND fast_swap_token_out_symbol = $1)))
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
    ) -> eyre::Result<Vec<signals::CrossChainSingleHop>> {
        let rows = sqlx::query_as!(
            SignalRow,
            r#"
            SELECT
                slow_chain, slow_height, slow_pool_id,
                fast_chain, fast_height, fast_pool_id,
                slow_swap_token_in_symbol, slow_swap_token_out_symbol,
                slow_swap_amount_in, slow_swap_amount_out,
                fast_swap_token_in_symbol, fast_swap_token_out_symbol,
                fast_swap_amount_in, fast_swap_amount_out,
                surplus_a, surplus_b, expected_profit_a, expected_profit_b,
                max_slippage_bps, congestion_risk_discount_bps
            FROM signals
            WHERE (((slow_swap_token_in_symbol = $1 AND slow_swap_token_out_symbol = $2)
                AND (fast_swap_token_in_symbol = $2 AND fast_swap_token_out_symbol = $1))
                OR ((fast_swap_token_in_symbol = $1 AND fast_swap_token_out_symbol = $2)
                AND (fast_swap_token_in_symbol = $1 AND fast_swap_token_out_symbol = $1)))
            ORDER BY fast_height DESC
            LIMIT $3 OFFSET $4
            "#,
            token_a_symbol,
            token_b_symbol,
            limit as i64,
            offset as i64
        )
        .fetch_all(&*self.pool)
        .await?;

        rows.into_iter()
            .map(|r| try_signal_from_row(r, &self.tokens_config))
            .collect()
    }
}

struct SignalRow {
    slow_chain: String,
    slow_height: i64,
    slow_pool_id: String,
    fast_chain: String,
    fast_height: i64,
    fast_pool_id: String,
    slow_swap_token_in_symbol: String,
    slow_swap_token_out_symbol: String,
    slow_swap_amount_in: String,
    slow_swap_amount_out: String,
    fast_swap_token_in_symbol: String,
    fast_swap_token_out_symbol: String,
    fast_swap_amount_in: String,
    fast_swap_amount_out: String,
    surplus_a: String,
    surplus_b: String,
    expected_profit_a: String,
    expected_profit_b: String,
    max_slippage_bps: i64,
    congestion_risk_discount_bps: i64,
}

fn try_signal_from_row(
    row: SignalRow,
    token_configs: &TokenAddressesForChain,
) -> eyre::Result<signals::CrossChainSingleHop> {
    unimplemented!()
}
