use std::{collections::HashMap, collections::HashSet, str::FromStr, sync::Arc};

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

use super::{SpotPriceRepository, try_chain_from_str, try_token_from_chain_symbol};

struct SignalRow {
    id: i64,
    // slow chain info
    slow_chain: String,
    slow_height: i64,
    // slow swap info
    slow_pool_id: String,
    slow_swap_token_in_symbol: String,
    slow_swap_token_out_symbol: String,
    slow_swap_amount_in: String,
    slow_swap_amount_out: String,
    slow_swap_gas_cost: String,
    // fast chain info
    fast_chain: String,
    fast_height: i64,
    // fast swap info
    fast_pool_id: String,
    fast_swap_token_in_symbol: String,
    fast_swap_token_out_symbol: String,
    fast_swap_amount_in: String,
    fast_swap_amount_out: String,
    fast_swap_gas_cost: String,
    // spot price FKs — only IDs; full data fetched separately via get_by_ids
    slow_prices_a_b_id: Option<i64>,
    slow_prices_a_usdc_id: Option<i64>,
    slow_prices_b_usdc_id: Option<i64>,
    slow_prices_eth_usdc_id: Option<i64>,
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
    // Signal config
    max_slippage_bps: i64,
    congestion_risk_discount_bps: i64,
    // Base fees
    slow_base_fee: i64,
    fast_base_fee: i64,
}

impl SignalRow {
    fn try_into_cross_chain_single_hop(
        self,
        token_configs: &TokenAddressesForChain,
        spot_prices: &HashMap<i64, SpotPrices>,
    ) -> eyre::Result<(i64, signals::CrossChainSingleHop)> {
        let id = self.id;
        let slow_chain = try_chain_from_str(&self.slow_chain, token_configs)
            .wrap_err("failed to parse slow chain from db")?;
        let fast_chain = try_chain_from_str(&self.fast_chain, token_configs)
            .wrap_err("failed to parse fast chain from db")?;

        let slow_swap_sim = try_swap_from_symbols_and_amounts(
            &self.slow_swap_token_in_symbol,
            &self.slow_swap_amount_in,
            &self.slow_swap_token_out_symbol,
            &self.slow_swap_amount_out,
            &self.slow_swap_gas_cost,
            &slow_chain,
            token_configs,
        )?;
        let slow_pair = Pair::new(
            slow_swap_sim.token_in.clone(),
            slow_swap_sim.token_out.clone(),
        );
        let slow_pool_id = PoolId::from(self.slow_pool_id.as_str());

        let fast_swap_sim = try_swap_from_symbols_and_amounts(
            &self.fast_swap_token_in_symbol,
            &self.fast_swap_amount_in,
            &self.fast_swap_token_out_symbol,
            &self.fast_swap_amount_out,
            &self.fast_swap_gas_cost,
            &fast_chain,
            token_configs,
        )?;
        let fast_pair = Pair::new(
            fast_swap_sim.token_in.clone(),
            fast_swap_sim.token_out.clone(),
        );
        let fast_pool_id = PoolId::from(self.fast_pool_id.as_str());

        let surplus = parse_biguint_pair(&self.surplus_a, &self.surplus_b, "surplus")?;
        let min_token_amounts = parse_biguint_pair(
            &self.min_token_amount_a,
            &self.min_token_amount_b,
            "min_token_amounts",
        )?;
        let min_usdc_amounts = parse_biguint_pair(
            &self.min_usdc_amount_a,
            &self.min_usdc_amount_b,
            "min_usdc_amounts",
        )?;
        let min_total_amount_usdc = BigUint::from_str(&self.min_total_amount_usdc)
            .map_err(|e| eyre!("failed to parse min_total_amount_usdc from db: {e:}"))?;
        let max_slippage_token_amounts = parse_biguint_pair(
            &self.max_slippage_token_amount_a,
            &self.max_slippage_token_amount_b,
            "max_slippage_token_amounts",
        )?;
        let gas_cost_eth = parse_biguint_pair(
            &self.gas_cost_eth_slow,
            &self.gas_cost_eth_fast,
            "gas_cost_eth",
        )?;
        let total_gas_cost_eth = BigUint::from_str(&self.total_gas_cost_eth)
            .map_err(|e| eyre!("failed to parse total_gas_cost_eth from db: {e:}"))?;
        let gas_cost_usdc = parse_biguint_pair(
            &self.gas_cost_usdc_slow,
            &self.gas_cost_usdc_fast,
            "gas_cost_usdc",
        )?;
        let total_gas_cost_usdc = BigUint::from_str(&self.total_gas_cost_usdc)
            .map_err(|e| eyre!("failed to parse total_gas_cost_usdc from db: {e:}"))?;

        let expected_profit = signals::ExpectedProfit {
            pair: slow_pair.clone(),
            surplus,
            max_slippage_token_amounts,
            min_token_amounts,
            token_usdc_prices: (self.token_usdc_price_a, self.token_usdc_price_b),
            min_usdc_amounts,
            min_total_amount_usdc,
            gas_cost_eth,
            total_gas_cost_eth,
            eth_usdc_price: self.eth_usdc_price,
            gas_cost_usdc,
            total_gas_cost_usdc,
        };

        // Spot prices looked up from the pre-fetched map by FK id
        let slow_prices_a_b = self
            .slow_prices_a_b_id
            .and_then(|id| spot_prices.get(&id).cloned())
            .unwrap_or_else(|| {
                // Fallback for legacy self written before spot price FK was added
                SpotPrices {
                    pair: slow_pair.clone(),
                    block_height: self.slow_height as u64,
                    min_price: 0.0,
                    max_price: 0.0,
                    min_pool_id: PoolId::from(self.slow_pool_id.as_str()),
                    max_pool_id: PoolId::from(self.slow_pool_id.as_str()),
                    chain: slow_chain.clone(),
                }
            });
        let slow_prices_a_usdc = self
            .slow_prices_a_usdc_id
            .and_then(|id| spot_prices.get(&id).cloned());
        let slow_prices_b_usdc = self
            .slow_prices_b_usdc_id
            .and_then(|id| spot_prices.get(&id).cloned());
        let slow_prices_eth_usdc = self
            .slow_prices_eth_usdc_id
            .and_then(|id| spot_prices.get(&id).cloned());

        Ok((id, signals::CrossChainSingleHop {
            slow_chain,
            slow_pair,
            slow_protocol_component: None,
            slow_height: self.slow_height as u64,
            slow_pool_id,
            slow_swap_sim,
            fast_chain,
            fast_pair,
            fast_protocol_component: None,
            fast_height: self.fast_height as u64,
            fast_pool_id,
            fast_swap_sim,
            slow_prices_a_b,
            slow_prices_a_usdc,
            slow_prices_b_usdc,
            slow_prices_eth_usdc,
            expected_profit,
            max_slippage_bps: self.max_slippage_bps as u64,
            congestion_risk_discount_bps: self.congestion_risk_discount_bps as u64,
            slow_base_fee: self.slow_base_fee as u64,
            fast_base_fee: self.fast_base_fee as u64,
        }))
    }
}

/// Collect all unique non-null spot price IDs from a batch of signal rows and
/// fetch them in a single query.
async fn fetch_spot_prices_for_rows(
    rows: &[SignalRow],
    repo: &SpotPriceRepository,
) -> eyre::Result<HashMap<i64, SpotPrices>> {
    let ids: Vec<i64> = rows
        .iter()
        .flat_map(|r| {
            [
                r.slow_prices_a_b_id,
                r.slow_prices_a_usdc_id,
                r.slow_prices_b_usdc_id,
                r.slow_prices_eth_usdc_id,
            ]
            .into_iter()
            .flatten()
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    repo.get_by_ids(&ids).await
}

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
    pub async fn insert(&self, signal: signals::CrossChainSingleHop) -> eyre::Result<i64> {
        let ep = &signal.expected_profit;
        let slow_chain = signal.slow_chain.name.to_string();
        let slow_height = signal.slow_height as i64;

        // CTE lookup symbols
        let a_usdc_token_a = signal
            .slow_prices_a_usdc
            .as_ref()
            .map(|p| p.pair.token_a().symbol.as_str());
        let a_usdc_token_b = signal
            .slow_prices_a_usdc
            .as_ref()
            .map(|p| p.pair.token_b().symbol.as_str());
        let b_usdc_token_a = signal
            .slow_prices_b_usdc
            .as_ref()
            .map(|p| p.pair.token_a().symbol.as_str());
        let b_usdc_token_b = signal
            .slow_prices_b_usdc
            .as_ref()
            .map(|p| p.pair.token_b().symbol.as_str());
        let eth_usdc_token_a = signal
            .slow_prices_eth_usdc
            .as_ref()
            .map(|p| p.pair.token_a().symbol.as_str());
        let eth_usdc_token_b = signal
            .slow_prices_eth_usdc
            .as_ref()
            .map(|p| p.pair.token_b().symbol.as_str());

        let row = sqlx::query!(
            r#"
            WITH
              sp_ab AS (
                SELECT id FROM spot_prices
                WHERE chain = $1 AND block_height = $2
                  AND token_a_symbol = $36 AND token_b_symbol = $37
                LIMIT 1
              ),
              sp_a_usdc AS (
                SELECT id FROM spot_prices
                WHERE chain = $1 AND block_height = $2
                  AND token_a_symbol = $38 AND token_b_symbol = $39
                LIMIT 1
              ),
              sp_b_usdc AS (
                SELECT id FROM spot_prices
                WHERE chain = $1 AND block_height = $2
                  AND token_a_symbol = $40 AND token_b_symbol = $41
                LIMIT 1
              ),
              sp_eth_usdc AS (
                SELECT id FROM spot_prices
                WHERE chain = $1 AND block_height = $2
                  AND token_a_symbol = $42 AND token_b_symbol = $43
                LIMIT 1
              )
            INSERT INTO signals (
                slow_chain, slow_height, slow_pool_id,
                slow_swap_token_in_symbol, slow_swap_token_out_symbol,
                slow_swap_amount_in, slow_swap_amount_out, slow_swap_gas_cost,
                fast_chain, fast_height, fast_pool_id,
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
            )
            SELECT
                $1,  $2,  $3,  $4,  $5,  $6,  $7,  $8,  $9,  $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
                $21, $22, $23, $24, $25, $26, $27, $28, $29, $30,
                $31, $32, $33, $34, $35, $44,
                (SELECT id FROM sp_ab),
                (SELECT id FROM sp_a_usdc),
                (SELECT id FROM sp_b_usdc),
                (SELECT id FROM sp_eth_usdc),
                $45, $46
            RETURNING id
            "#,
            &slow_chain,                                   // $1
            slow_height,                                   // $2
            &signal.slow_pool_id.to_string(),              // $3
            &signal.slow_swap_sim.token_in.symbol,         // $4
            &signal.slow_swap_sim.token_out.symbol,        // $5
            &signal.slow_swap_sim.amount_in.to_string(),   // $6
            &signal.slow_swap_sim.amount_out.to_string(),  // $7
            &signal.slow_swap_sim.gas_cost.to_string(),    // $8
            &signal.fast_chain.name.to_string(),           // $9
            signal.fast_height as i64,                     // $10
            &signal.fast_pool_id.to_string(),              // $11
            &signal.fast_swap_sim.token_in.symbol,         // $12
            &signal.fast_swap_sim.token_out.symbol,        // $13
            &signal.fast_swap_sim.amount_in.to_string(),   // $14
            &signal.fast_swap_sim.amount_out.to_string(),  // $15
            &signal.fast_swap_sim.gas_cost.to_string(),    // $16
            &ep.surplus.0.to_string(),                     // $17
            &ep.surplus.1.to_string(),                     // $18
            &ep.min_token_amounts.0.to_string(),           // $19
            &ep.min_token_amounts.1.to_string(),           // $20
            &ep.min_usdc_amounts.0.to_string(),            // $21
            &ep.min_usdc_amounts.1.to_string(),            // $22
            &ep.min_total_amount_usdc.to_string(),         // $23
            &ep.max_slippage_token_amounts.0.to_string(),  // $24
            &ep.max_slippage_token_amounts.1.to_string(),  // $25
            ep.token_usdc_prices.0,                        // $26
            ep.token_usdc_prices.1,                        // $27
            &ep.gas_cost_eth.0.to_string(),                // $28
            &ep.gas_cost_eth.1.to_string(),                // $29
            &ep.total_gas_cost_eth.to_string(),            // $30
            ep.eth_usdc_price,                             // $31
            &ep.gas_cost_usdc.0.to_string(),               // $32
            &ep.gas_cost_usdc.1.to_string(),               // $33
            &ep.total_gas_cost_usdc.to_string(),           // $34
            signal.slow_base_fee as i64,                   // $35
            &signal.slow_prices_a_b.pair.token_a().symbol, // $36
            &signal.slow_prices_a_b.pair.token_b().symbol, // $37
            a_usdc_token_a,                                // $38
            a_usdc_token_b,                                // $39
            b_usdc_token_a,                                // $40
            b_usdc_token_b,                                // $41
            eth_usdc_token_a,                              // $42
            eth_usdc_token_b,                              // $43
            signal.fast_base_fee as i64,                   // $44
            signal.max_slippage_bps as i64,                // $45
            signal.congestion_risk_discount_bps as i64,    // $46
        )
        .fetch_one(self.pool.as_ref())
        .await?;

        Ok(row.id)
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
            WHERE (
                (slow_swap_token_in_symbol = $1 AND slow_swap_token_out_symbol = $2)
                OR (slow_swap_token_in_symbol = $2 AND slow_swap_token_out_symbol = $1)
            )
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
        spot_price_repo: &SpotPriceRepository,
    ) -> eyre::Result<Vec<(i64, signals::CrossChainSingleHop)>> {
        let rows: Vec<SignalRow> = sqlx::query_as!(
            SignalRow,
            r#"
            SELECT
                id,
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
            FROM signals
            WHERE (
                (slow_swap_token_in_symbol = $1 AND slow_swap_token_out_symbol = $2)
                OR (slow_swap_token_in_symbol = $2 AND slow_swap_token_out_symbol = $1)
            )
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#,
            token_a_symbol,
            token_b_symbol,
            limit as i64,
            offset as i64
        )
        .fetch_all(&*self.pool)
        .await?;

        let spot_prices = fetch_spot_prices_for_rows(&rows, spot_price_repo).await?;

        rows.into_iter()
            .map(|r| r.try_into_cross_chain_single_hop(&self.tokens_config, &spot_prices))
            .collect()
    }

    pub async fn get_by_id(
        &self,
        signal_id: i64,
        spot_price_repo: &SpotPriceRepository,
    ) -> eyre::Result<Option<signals::CrossChainSingleHop>> {
        let row: Option<SignalRow> = sqlx::query_as!(
            SignalRow,
            r#"
            SELECT
                id,
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
            FROM signals
            WHERE id = $1
            "#,
            signal_id,
        )
        .fetch_optional(&*self.pool)
        .await?;

        match row {
            Some(r) => {
                let rows = std::slice::from_ref(&r);
                let spot_prices = fetch_spot_prices_for_rows(rows, spot_price_repo).await?;
                Some(r.try_into_cross_chain_single_hop(&self.tokens_config, &spot_prices).map(|(_, s)| s))
                    .transpose()
            }
            None => Ok(None),
        }
    }
}

fn parse_biguint_pair(a: &str, b: &str, field: &str) -> eyre::Result<(BigUint, BigUint)> {
    let a = BigUint::from_str(a).map_err(|e| eyre!("failed to parse {field}.0 from db: {e:}"))?;
    let b = BigUint::from_str(b).map_err(|e| eyre!("failed to parse {field}.1 from db: {e:}"))?;
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
