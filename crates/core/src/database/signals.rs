use std::{str::FromStr, sync::Arc};

use color_eyre::eyre::{self, Context, eyre};
use num_bigint::BigUint;
use sqlx::PgPool;
use tracing::instrument;

use crate::{
    chain::Chain,
    config::TokenAddressesForChain,
    signals,
    spot_prices::SpotPrices,
    state::{PoolId, pair::Pair},
    strategy::Swap,
};

use super::{try_chain_from_str, try_token_from_chain_symbol};

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
    pub async fn insert(
        &self,
        signal: signals::CrossChainSingleHop,
        slow_prices_a_b_id: Option<i64>,
        slow_prices_a_usdc_id: Option<i64>,
        slow_prices_b_usdc_id: Option<i64>,
        slow_prices_eth_usdc_id: Option<i64>,
    ) -> eyre::Result<i64> {
        let ep = &signal.expected_profit;
        let id = sqlx::query!(
            r#"
            INSERT INTO signals (
                slow_chain, slow_height, slow_pool_id,
                fast_chain, fast_height, fast_pool_id,
                slow_swap_token_in_symbol, slow_swap_token_out_symbol,
                slow_swap_amount_in, slow_swap_amount_out, slow_swap_gas_cost,
                fast_swap_token_in_symbol, fast_swap_token_out_symbol,
                fast_swap_amount_in, fast_swap_amount_out, fast_swap_gas_cost,
                surplus_a, surplus_b,
                min_token_amount_a, min_token_amount_b,
                min_usdc_amount_a, min_usdc_amount_b, min_total_amount_usdc,
                max_slippage_token_amount_a, max_slippage_token_amount_b,
                token_usdc_price_a, token_usdc_price_b,
                gas_cost_eth_slow, gas_cost_eth_fast, total_gas_cost_eth,
                eth_usdc_price,
                gas_cost_usdc_slow, gas_cost_usdc_fast, total_gas_cost_usdc,
                slow_base_fee, fast_base_fee,
                slow_prices_a_b_id, slow_prices_a_usdc_id,
                slow_prices_b_usdc_id, slow_prices_eth_usdc_id,
                max_slippage_bps, congestion_risk_discount_bps
            ) VALUES (
                $1,  $2,  $3,  $4,  $5,  $6,  $7,  $8,  $9,  $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
                $21, $22, $23, $24, $25, $26, $27, $28, $29, $30,
                $31, $32, $33, $34, $35, $36, $37, $38, $39, $40,
                $41, $42
            )
            RETURNING id
            "#,
            &signal.slow_chain.name.to_string(),           // $1
            signal.slow_height as i64,                      // $2
            &signal.slow_pool_id.to_string(),               // $3
            &signal.fast_chain.name.to_string(),            // $4
            signal.fast_height as i64,                      // $5
            &signal.fast_pool_id.to_string(),               // $6
            &signal.slow_swap_sim.token_in.symbol,          // $7
            &signal.slow_swap_sim.token_out.symbol,         // $8
            &signal.slow_swap_sim.amount_in.to_string(),    // $9
            &signal.slow_swap_sim.amount_out.to_string(),   // $10
            &signal.slow_swap_sim.gas_cost.to_string(),     // $11
            &signal.fast_swap_sim.token_in.symbol,          // $12
            &signal.fast_swap_sim.token_out.symbol,         // $13
            &signal.fast_swap_sim.amount_in.to_string(),    // $14
            &signal.fast_swap_sim.amount_out.to_string(),   // $15
            &signal.fast_swap_sim.gas_cost.to_string(),     // $16
            &ep.surplus.0.to_string(),                      // $17
            &ep.surplus.1.to_string(),                      // $18
            &ep.min_token_amounts.0.to_string(),            // $19
            &ep.min_token_amounts.1.to_string(),            // $20
            &ep.min_usdc_amounts.0.to_string(),             // $21
            &ep.min_usdc_amounts.1.to_string(),             // $22
            &ep.min_total_amount_usdc.to_string(),          // $23
            &ep.max_slippage_token_amounts.0.to_string(),   // $24
            &ep.max_slippage_token_amounts.1.to_string(),   // $25
            ep.token_usdc_prices.0,                         // $26
            ep.token_usdc_prices.1,                         // $27
            &ep.gas_cost_eth.0.to_string(),                 // $28
            &ep.gas_cost_eth.1.to_string(),                 // $29
            &ep.total_gas_cost_eth.to_string(),             // $30
            ep.eth_usdc_price,                              // $31
            &ep.gas_cost_usdc.0.to_string(),                // $32
            &ep.gas_cost_usdc.1.to_string(),                // $33
            &ep.total_gas_cost_usdc.to_string(),            // $34
            signal.slow_base_fee as i64,                    // $35
            signal.fast_base_fee as i64,                    // $36
            slow_prices_a_b_id,                             // $37
            slow_prices_a_usdc_id,                          // $38
            slow_prices_b_usdc_id,                          // $39
            slow_prices_eth_usdc_id,                        // $40
            signal.max_slippage_bps as i64,                 // $41
            signal.congestion_risk_discount_bps as i64,     // $42
        )
        .fetch_one(self.pool.as_ref())
        .await?
        .id;

        Ok(id)
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
            WHERE (((slow_swap_token_in_symbol = $1 AND slow_swap_token_out_symbol = $2)
                AND (fast_swap_token_in_symbol = $2 AND fast_swap_token_out_symbol = $1))
                OR ((fast_swap_token_in_symbol = $1 AND fast_swap_token_out_symbol = $2)
                AND (fast_swap_token_in_symbol = $2 AND fast_swap_token_out_symbol = $1)))
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
        let rows: Vec<SignalRow> = sqlx::query_as!(
            SignalRow,
            r#"
            SELECT
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
                s.max_slippage_bps, s.congestion_risk_discount_bps,
                -- slow_prices_a_b (required)
                p_ab.token_a_symbol  AS "sp_ab_token_a_symbol?",
                p_ab.token_b_symbol  AS "sp_ab_token_b_symbol?",
                p_ab.block_height    AS "sp_ab_block_height?",
                p_ab.min_price       AS "sp_ab_min_price?",
                p_ab.max_price       AS "sp_ab_max_price?",
                p_ab.min_pool_id     AS "sp_ab_min_pool_id?",
                p_ab.max_pool_id     AS "sp_ab_max_pool_id?",
                p_ab.chain           AS "sp_ab_chain?",
                -- slow_prices_a_usdc (optional)
                p_ausdc.token_a_symbol AS "sp_ausdc_token_a_symbol?",
                p_ausdc.token_b_symbol AS "sp_ausdc_token_b_symbol?",
                p_ausdc.block_height   AS "sp_ausdc_block_height?",
                p_ausdc.min_price      AS "sp_ausdc_min_price?",
                p_ausdc.max_price      AS "sp_ausdc_max_price?",
                p_ausdc.min_pool_id    AS "sp_ausdc_min_pool_id?",
                p_ausdc.max_pool_id    AS "sp_ausdc_max_pool_id?",
                p_ausdc.chain          AS "sp_ausdc_chain?",
                -- slow_prices_b_usdc (optional)
                p_busdc.token_a_symbol AS "sp_busdc_token_a_symbol?",
                p_busdc.token_b_symbol AS "sp_busdc_token_b_symbol?",
                p_busdc.block_height   AS "sp_busdc_block_height?",
                p_busdc.min_price      AS "sp_busdc_min_price?",
                p_busdc.max_price      AS "sp_busdc_max_price?",
                p_busdc.min_pool_id    AS "sp_busdc_min_pool_id?",
                p_busdc.max_pool_id    AS "sp_busdc_max_pool_id?",
                p_busdc.chain          AS "sp_busdc_chain?",
                -- slow_prices_eth_usdc (optional)
                p_eth.token_a_symbol AS "sp_eth_token_a_symbol?",
                p_eth.token_b_symbol AS "sp_eth_token_b_symbol?",
                p_eth.block_height   AS "sp_eth_block_height?",
                p_eth.min_price      AS "sp_eth_min_price?",
                p_eth.max_price      AS "sp_eth_max_price?",
                p_eth.min_pool_id    AS "sp_eth_min_pool_id?",
                p_eth.max_pool_id    AS "sp_eth_max_pool_id?",
                p_eth.chain          AS "sp_eth_chain?"
            FROM signals s
            LEFT JOIN spot_prices p_ab    ON p_ab.id    = s.slow_prices_a_b_id
            LEFT JOIN spot_prices p_ausdc ON p_ausdc.id = s.slow_prices_a_usdc_id
            LEFT JOIN spot_prices p_busdc ON p_busdc.id = s.slow_prices_b_usdc_id
            LEFT JOIN spot_prices p_eth   ON p_eth.id   = s.slow_prices_eth_usdc_id
            WHERE (((s.slow_swap_token_in_symbol = $1 AND s.slow_swap_token_out_symbol = $2)
                AND (s.fast_swap_token_in_symbol = $2 AND s.fast_swap_token_out_symbol = $1))
                OR ((s.slow_swap_token_in_symbol = $2 AND s.slow_swap_token_out_symbol = $1)
                AND (s.fast_swap_token_in_symbol = $1 AND s.fast_swap_token_out_symbol = $2)))
            ORDER BY s.created_at DESC
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

    pub async fn get_by_id(
        &self,
        signal_id: i64,
    ) -> eyre::Result<Option<signals::CrossChainSingleHop>> {
        let row: Option<SignalRow> = sqlx::query_as!(
            SignalRow,
            r#"
            SELECT
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
                s.max_slippage_bps, s.congestion_risk_discount_bps,
                p_ab.token_a_symbol  AS "sp_ab_token_a_symbol?",
                p_ab.token_b_symbol  AS "sp_ab_token_b_symbol?",
                p_ab.block_height    AS "sp_ab_block_height?",
                p_ab.min_price       AS "sp_ab_min_price?",
                p_ab.max_price       AS "sp_ab_max_price?",
                p_ab.min_pool_id     AS "sp_ab_min_pool_id?",
                p_ab.max_pool_id     AS "sp_ab_max_pool_id?",
                p_ab.chain           AS "sp_ab_chain?",
                p_ausdc.token_a_symbol AS "sp_ausdc_token_a_symbol?",
                p_ausdc.token_b_symbol AS "sp_ausdc_token_b_symbol?",
                p_ausdc.block_height   AS "sp_ausdc_block_height?",
                p_ausdc.min_price      AS "sp_ausdc_min_price?",
                p_ausdc.max_price      AS "sp_ausdc_max_price?",
                p_ausdc.min_pool_id    AS "sp_ausdc_min_pool_id?",
                p_ausdc.max_pool_id    AS "sp_ausdc_max_pool_id?",
                p_ausdc.chain          AS "sp_ausdc_chain?",
                p_busdc.token_a_symbol AS "sp_busdc_token_a_symbol?",
                p_busdc.token_b_symbol AS "sp_busdc_token_b_symbol?",
                p_busdc.block_height   AS "sp_busdc_block_height?",
                p_busdc.min_price      AS "sp_busdc_min_price?",
                p_busdc.max_price      AS "sp_busdc_max_price?",
                p_busdc.min_pool_id    AS "sp_busdc_min_pool_id?",
                p_busdc.max_pool_id    AS "sp_busdc_max_pool_id?",
                p_busdc.chain          AS "sp_busdc_chain?",
                p_eth.token_a_symbol AS "sp_eth_token_a_symbol?",
                p_eth.token_b_symbol AS "sp_eth_token_b_symbol?",
                p_eth.block_height   AS "sp_eth_block_height?",
                p_eth.min_price      AS "sp_eth_min_price?",
                p_eth.max_price      AS "sp_eth_max_price?",
                p_eth.min_pool_id    AS "sp_eth_min_pool_id?",
                p_eth.max_pool_id    AS "sp_eth_max_pool_id?",
                p_eth.chain          AS "sp_eth_chain?"
            FROM signals s
            LEFT JOIN spot_prices p_ab    ON p_ab.id    = s.slow_prices_a_b_id
            LEFT JOIN spot_prices p_ausdc ON p_ausdc.id = s.slow_prices_a_usdc_id
            LEFT JOIN spot_prices p_busdc ON p_busdc.id = s.slow_prices_b_usdc_id
            LEFT JOIN spot_prices p_eth   ON p_eth.id   = s.slow_prices_eth_usdc_id
            WHERE s.id = $1
            "#,
            signal_id,
        )
        .fetch_optional(&*self.pool)
        .await?;

        row.map(|r| try_signal_from_row(r, &self.tokens_config))
            .transpose()
    }
}

struct SignalRow {
    // Core signal fields
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
    slow_swap_gas_cost: String,
    fast_swap_token_in_symbol: String,
    fast_swap_token_out_symbol: String,
    fast_swap_amount_in: String,
    fast_swap_amount_out: String,
    fast_swap_gas_cost: String,
    // ExpectedProfit fields
    surplus_a: String,
    surplus_b: String,
    min_token_amount_a: String,
    min_token_amount_b: String,
    min_usdc_amount_a: String,
    min_usdc_amount_b: String,
    min_total_amount_usdc: String,
    max_slippage_token_amount_a: String,
    max_slippage_token_amount_b: String,
    token_usdc_price_a: f64,
    token_usdc_price_b: f64,
    gas_cost_eth_slow: String,
    gas_cost_eth_fast: String,
    total_gas_cost_eth: String,
    eth_usdc_price: f64,
    gas_cost_usdc_slow: String,
    gas_cost_usdc_fast: String,
    total_gas_cost_usdc: String,
    // Base fees
    slow_base_fee: i64,
    fast_base_fee: i64,
    // Signal config
    max_slippage_bps: i64,
    congestion_risk_discount_bps: i64,
    // Joined spot price fields — slow_prices_a_b (nullable because FK may be absent)
    sp_ab_token_a_symbol: Option<String>,
    sp_ab_token_b_symbol: Option<String>,
    sp_ab_block_height: Option<i64>,
    sp_ab_min_price: Option<f64>,
    sp_ab_max_price: Option<f64>,
    sp_ab_min_pool_id: Option<String>,
    sp_ab_max_pool_id: Option<String>,
    sp_ab_chain: Option<String>,
    // slow_prices_a_usdc
    sp_ausdc_token_a_symbol: Option<String>,
    sp_ausdc_token_b_symbol: Option<String>,
    sp_ausdc_block_height: Option<i64>,
    sp_ausdc_min_price: Option<f64>,
    sp_ausdc_max_price: Option<f64>,
    sp_ausdc_min_pool_id: Option<String>,
    sp_ausdc_max_pool_id: Option<String>,
    sp_ausdc_chain: Option<String>,
    // slow_prices_b_usdc
    sp_busdc_token_a_symbol: Option<String>,
    sp_busdc_token_b_symbol: Option<String>,
    sp_busdc_block_height: Option<i64>,
    sp_busdc_min_price: Option<f64>,
    sp_busdc_max_price: Option<f64>,
    sp_busdc_min_pool_id: Option<String>,
    sp_busdc_max_pool_id: Option<String>,
    sp_busdc_chain: Option<String>,
    // slow_prices_eth_usdc
    sp_eth_token_a_symbol: Option<String>,
    sp_eth_token_b_symbol: Option<String>,
    sp_eth_block_height: Option<i64>,
    sp_eth_min_price: Option<f64>,
    sp_eth_max_price: Option<f64>,
    sp_eth_min_pool_id: Option<String>,
    sp_eth_max_pool_id: Option<String>,
    sp_eth_chain: Option<String>,
}

/// Try to reconstruct a `SpotPrices` from the optional joined columns for one price series.
/// Returns `None` if the FK was NULL (no joined row).
fn try_spot_prices_from_row_cols(
    token_a_symbol: Option<String>,
    token_b_symbol: Option<String>,
    block_height: Option<i64>,
    min_price: Option<f64>,
    max_price: Option<f64>,
    min_pool_id: Option<String>,
    max_pool_id: Option<String>,
    chain_name: Option<String>,
    token_configs: &TokenAddressesForChain,
) -> eyre::Result<Option<SpotPrices>> {
    match (
        token_a_symbol,
        token_b_symbol,
        block_height,
        min_price,
        max_price,
        min_pool_id,
        max_pool_id,
        chain_name,
    ) {
        (
            Some(ta),
            Some(tb),
            Some(height),
            Some(min_p),
            Some(max_p),
            Some(min_pid),
            Some(max_pid),
            Some(chain_str),
        ) => {
            let chain = try_chain_from_str(&chain_str, token_configs)?;
            let token_a = try_token_from_chain_symbol(&ta, &chain, token_configs)
                .map_err(|e| eyre!("failed to parse spot price token_a: {e:}"))?;
            let token_b = try_token_from_chain_symbol(&tb, &chain, token_configs)
                .map_err(|e| eyre!("failed to parse spot price token_b: {e:}"))?;
            Ok(Some(SpotPrices {
                pair: Pair::new(token_a, token_b),
                block_height: height as u64,
                min_price: min_p,
                max_price: max_p,
                min_pool_id: PoolId::from(min_pid.as_str()),
                max_pool_id: PoolId::from(max_pid.as_str()),
                chain,
            }))
        }
        // FK was NULL — all columns will be NULL
        _ => Ok(None),
    }
}

fn try_signal_from_row(
    row: SignalRow,
    token_configs: &TokenAddressesForChain,
) -> eyre::Result<signals::CrossChainSingleHop> {
    let slow_chain = try_chain_from_str(&row.slow_chain, token_configs)
        .wrap_err("failed to parse slow chain from db")?;
    let fast_chain = try_chain_from_str(&row.fast_chain, token_configs)
        .wrap_err("failed to parse fast chain from db")?;

    let slow_swap_sim = try_swap_from_symbols_and_amounts(
        &row.slow_swap_token_in_symbol,
        &row.slow_swap_amount_in,
        &row.slow_swap_token_out_symbol,
        &row.slow_swap_amount_out,
        &row.slow_swap_gas_cost,
        &slow_chain,
        token_configs,
    )?;
    let slow_pair = Pair::new(
        slow_swap_sim.token_in.clone(),
        slow_swap_sim.token_out.clone(),
    );
    let slow_pool_id = PoolId::from(row.slow_pool_id.as_str());

    let fast_swap_sim = try_swap_from_symbols_and_amounts(
        &row.fast_swap_token_in_symbol,
        &row.fast_swap_amount_in,
        &row.fast_swap_token_out_symbol,
        &row.fast_swap_amount_out,
        &row.fast_swap_gas_cost,
        &fast_chain,
        token_configs,
    )?;
    let fast_pair = Pair::new(
        fast_swap_sim.token_in.clone(),
        fast_swap_sim.token_out.clone(),
    );
    let fast_pool_id = PoolId::from(row.fast_pool_id.as_str());

    // --- ExpectedProfit ---
    let surplus = parse_biguint_pair(&row.surplus_a, &row.surplus_b, "surplus")?;
    let min_token_amounts = parse_biguint_pair(
        &row.min_token_amount_a,
        &row.min_token_amount_b,
        "min_token_amounts",
    )?;
    let min_usdc_amounts = parse_biguint_pair(
        &row.min_usdc_amount_a,
        &row.min_usdc_amount_b,
        "min_usdc_amounts",
    )?;
    let min_total_amount_usdc = BigUint::from_str(&row.min_total_amount_usdc)
        .map_err(|e| eyre!("failed to parse min_total_amount_usdc from db: {e:}"))?;
    let max_slippage_token_amounts = parse_biguint_pair(
        &row.max_slippage_token_amount_a,
        &row.max_slippage_token_amount_b,
        "max_slippage_token_amounts",
    )?;
    let gas_cost_eth = parse_biguint_pair(
        &row.gas_cost_eth_slow,
        &row.gas_cost_eth_fast,
        "gas_cost_eth",
    )?;
    let total_gas_cost_eth = BigUint::from_str(&row.total_gas_cost_eth)
        .map_err(|e| eyre!("failed to parse total_gas_cost_eth from db: {e:}"))?;
    let gas_cost_usdc = parse_biguint_pair(
        &row.gas_cost_usdc_slow,
        &row.gas_cost_usdc_fast,
        "gas_cost_usdc",
    )?;
    let total_gas_cost_usdc = BigUint::from_str(&row.total_gas_cost_usdc)
        .map_err(|e| eyre!("failed to parse total_gas_cost_usdc from db: {e:}"))?;

    let expected_profit = signals::ExpectedProfit {
        pair: slow_pair.clone(),
        surplus,
        max_slippage_token_amounts,
        min_token_amounts,
        token_usdc_prices: (row.token_usdc_price_a, row.token_usdc_price_b),
        min_usdc_amounts,
        min_total_amount_usdc,
        gas_cost_eth,
        total_gas_cost_eth,
        eth_usdc_price: row.eth_usdc_price,
        gas_cost_usdc,
        total_gas_cost_usdc,
    };

    // --- Spot prices (reconstructed from LEFT JOIN columns) ---
    let slow_prices_a_b = try_spot_prices_from_row_cols(
        row.sp_ab_token_a_symbol,
        row.sp_ab_token_b_symbol,
        row.sp_ab_block_height,
        row.sp_ab_min_price,
        row.sp_ab_max_price,
        row.sp_ab_min_pool_id,
        row.sp_ab_max_pool_id,
        row.sp_ab_chain,
        token_configs,
    )?
    .unwrap_or_else(|| {
        // Fallback for legacy rows written before spot price FK was added
        SpotPrices {
            pair: slow_pair.clone(),
            block_height: row.slow_height as u64,
            min_price: 0.0,
            max_price: 0.0,
            min_pool_id: PoolId::from(row.slow_pool_id.as_str()),
            max_pool_id: PoolId::from(row.slow_pool_id.as_str()),
            chain: slow_chain.clone(),
        }
    });

    let slow_prices_a_usdc = try_spot_prices_from_row_cols(
        row.sp_ausdc_token_a_symbol,
        row.sp_ausdc_token_b_symbol,
        row.sp_ausdc_block_height,
        row.sp_ausdc_min_price,
        row.sp_ausdc_max_price,
        row.sp_ausdc_min_pool_id,
        row.sp_ausdc_max_pool_id,
        row.sp_ausdc_chain,
        token_configs,
    )?;

    let slow_prices_b_usdc = try_spot_prices_from_row_cols(
        row.sp_busdc_token_a_symbol,
        row.sp_busdc_token_b_symbol,
        row.sp_busdc_block_height,
        row.sp_busdc_min_price,
        row.sp_busdc_max_price,
        row.sp_busdc_min_pool_id,
        row.sp_busdc_max_pool_id,
        row.sp_busdc_chain,
        token_configs,
    )?;

    let slow_prices_eth_usdc = try_spot_prices_from_row_cols(
        row.sp_eth_token_a_symbol,
        row.sp_eth_token_b_symbol,
        row.sp_eth_block_height,
        row.sp_eth_min_price,
        row.sp_eth_max_price,
        row.sp_eth_min_pool_id,
        row.sp_eth_max_pool_id,
        row.sp_eth_chain,
        token_configs,
    )?;

    Ok(signals::CrossChainSingleHop {
        slow_chain,
        slow_pair,
        slow_protocol_component: None, // not stored in db
        slow_height: row.slow_height as u64,
        slow_pool_id,
        slow_swap_sim,
        fast_chain,
        fast_pair,
        fast_protocol_component: None, // not stored in db
        fast_height: row.fast_height as u64,
        fast_pool_id,
        fast_swap_sim,
        slow_prices_a_b,
        slow_prices_a_usdc,
        slow_prices_b_usdc,
        slow_prices_eth_usdc,
        expected_profit,
        max_slippage_bps: row.max_slippage_bps as u64,
        congestion_risk_discount_bps: row.congestion_risk_discount_bps as u64,
        slow_base_fee: row.slow_base_fee as u64,
        fast_base_fee: row.fast_base_fee as u64,
    })
}

fn parse_biguint_pair(a: &str, b: &str, field: &str) -> eyre::Result<(BigUint, BigUint)> {
    let a = BigUint::from_str(a)
        .map_err(|e| eyre!("failed to parse {field}.0 from db: {e:}"))?;
    let b = BigUint::from_str(b)
        .map_err(|e| eyre!("failed to parse {field}.1 from db: {e:}"))?;
    Ok((a, b))
}

fn try_swap_from_symbols_and_amounts(
    token_in_symbol: &str,
    token_in_amount: &str,
    token_out_symbol: &str,
    token_out_amount: &str,
    gas_cost: &str,
    chain: &Chain,
    token_configs: &TokenAddressesForChain,
) -> eyre::Result<Swap> {
    let token_in = try_token_from_chain_symbol(token_in_symbol, chain, token_configs)
        .map_err(|e| eyre!("failed to parse token_in: {e:}"))?;
    let amount_in =
        BigUint::from_str(token_in_amount).map_err(|e| eyre!("failed to parse amount_in: {e:}"))?;

    let token_out = try_token_from_chain_symbol(token_out_symbol, chain, token_configs)
        .map_err(|e| eyre!("failed to parse token_out: {e:}"))?;
    let amount_out = BigUint::from_str(token_out_amount)
        .map_err(|e| eyre!("failed to parse amount_out: {e:}"))?;

    let gas_cost =
        BigUint::from_str(gas_cost).map_err(|e| eyre!("failed to parse gas_cost: {e:}"))?;

    Ok(Swap {
        token_in,
        token_out,
        amount_in,
        amount_out,
        gas_cost,
    })
}
