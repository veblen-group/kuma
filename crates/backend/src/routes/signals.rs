//! HTTP handlers for `GET /signals` and `GET /signals/:id`.
//!
//! Defines three layered response types that mirror the core signal types but
//! strip out any credential-bearing fields:
//! - [`SwapResponse`] — one leg of an arb (chain name, height, pool, amounts).
//! - [`ExpectedProfitResponse`] — all profit fields with `BigUint` serialized as
//!   decimal strings to avoid JSON precision loss.
//! - [`CrossChainSingleHopResponse`] — the full signal, combining both legs,
//!   spot prices, and expected profit. Also carries `fast_prices_a_b_created_at`
//!   for the webapp timeline display.
//!
//! Spot price hydration is done inside `SignalRepository` (two-phase batch fetch);
//! the route layer only maps DB results to response types.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use kuma_core::signals::{CrossChainSingleHop, ExpectedProfit};
use sqlx::types::chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    models::{PaginatedResponse, PaginationQuery},
    pair::parse_pair,
    routes::spot_prices::SpotPriceResponse,
    AppState,
};

/// One side of a cross-chain arbitrage — chain context plus the simulated swap.
/// Used for both the slow and fast legs in `CrossChainSingleHopResponse`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SwapResponse {
    pub chain: String,
    pub height: u64,
    pub token_in: String,
    pub token_out: String,
    pub pool_id: String,
    pub amount_in: String,
    pub amount_out: String,
    pub base_fee: u64,
    pub gas_cost: String,
}

/// API response type for `ExpectedProfit`. All `BigUint` fields are serialized
/// as decimal strings to avoid precision loss.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExpectedProfitResponse {
    pub token_a: String,
    pub token_b: String,
    pub surplus_a: String,
    pub surplus_b: String,
    pub max_slippage_token_amount_a: String,
    pub max_slippage_token_amount_b: String,
    pub min_token_amount_a: String,
    pub min_token_amount_b: String,
    pub token_usdc_price_a: f64,
    pub token_usdc_price_b: f64,
    pub min_usdc_amount_a: String,
    pub min_usdc_amount_b: String,
    pub min_total_amount_usdc: String,
    pub gas_cost_eth_slow: String,
    pub gas_cost_eth_fast: String,
    pub total_gas_cost_eth: String,
    pub eth_usdc_price: f64,
    pub gas_cost_usdc_slow: String,
    pub gas_cost_usdc_fast: String,
    pub total_gas_cost_usdc: String,
}

impl From<ExpectedProfit> for ExpectedProfitResponse {
    fn from(ep: ExpectedProfit) -> Self {
        Self {
            token_a: ep.pair.token_a().symbol.clone(),
            token_b: ep.pair.token_b().symbol.clone(),
            surplus_a: ep.surplus.0.to_string(),
            surplus_b: ep.surplus.1.to_string(),
            max_slippage_token_amount_a: ep.max_slippage_token_amounts.0.to_string(),
            max_slippage_token_amount_b: ep.max_slippage_token_amounts.1.to_string(),
            min_token_amount_a: ep.min_token_amounts.0.to_string(),
            min_token_amount_b: ep.min_token_amounts.1.to_string(),
            token_usdc_price_a: ep.token_usdc_prices.0,
            token_usdc_price_b: ep.token_usdc_prices.1,
            min_usdc_amount_a: ep.min_usdc_amounts.0.to_string(),
            min_usdc_amount_b: ep.min_usdc_amounts.1.to_string(),
            min_total_amount_usdc: ep.min_total_amount_usdc.to_string(),
            gas_cost_eth_slow: ep.gas_cost_eth.0.to_string(),
            gas_cost_eth_fast: ep.gas_cost_eth.1.to_string(),
            total_gas_cost_eth: ep.total_gas_cost_eth.to_string(),
            eth_usdc_price: ep.eth_usdc_price,
            gas_cost_usdc_slow: ep.gas_cost_usdc.0.to_string(),
            gas_cost_usdc_fast: ep.gas_cost_usdc.1.to_string(),
            total_gas_cost_usdc: ep.total_gas_cost_usdc.to_string(),
        }
    }
}

/// API response type for `CrossChainSingleHop`. Chain objects are replaced with
/// safe `SwapResponse` values (name strings only — no API keys or RPC URLs).
/// `protocol_component` is omitted (internal encoding detail).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CrossChainSingleHopResponse {
    pub id: i64,
    pub slow: SwapResponse,
    pub fast: SwapResponse,
    /// Spot prices on the slow chain at signal generation time.
    /// `None` for signals written before spot price FK tracking was added.
    pub slow_prices_a_b: SpotPriceResponse,
    pub slow_prices_a_usdc: Option<SpotPriceResponse>,
    pub slow_prices_b_usdc: Option<SpotPriceResponse>,
    pub slow_prices_eth_usdc: Option<SpotPriceResponse>,
    pub expected_profit: ExpectedProfitResponse,
    pub max_slippage_bps: u64,
    pub congestion_risk_discount_bps: u64,
    /// Timestamp of the fast chain spot price at signal generation time.
    pub fast_prices_a_b_created_at: Option<String>,
}

impl CrossChainSingleHopResponse {
    pub fn new(id: i64, s: CrossChainSingleHop, slow_prices_a_b_created_at: Option<DateTime<Utc>>, fast_prices_a_b_created_at: Option<DateTime<Utc>>) -> Self {
        let mut slow_prices_a_b = SpotPriceResponse::from(s.slow_prices_a_b);
        slow_prices_a_b.created_at = slow_prices_a_b_created_at.map(|dt| dt.to_rfc3339());
        Self {
            id,
            slow: SwapResponse {
                chain: s.slow_chain.name.to_string(),
                height: s.slow_height,
                pool_id: s.slow_pool_id.to_string(),
                base_fee: s.slow_base_fee,
                token_in: s.slow_swap_sim.token_in.symbol.clone(),
                token_out: s.slow_swap_sim.token_out.symbol.clone(),
                amount_in: s.slow_swap_sim.amount_in.to_string(),
                amount_out: s.slow_swap_sim.amount_out.to_string(),
                gas_cost: s.slow_swap_sim.gas_cost.to_string(),
            },
            fast: SwapResponse {
                chain: s.fast_chain.name.to_string(),
                height: s.fast_height,
                pool_id: s.fast_pool_id.to_string(),
                base_fee: s.fast_base_fee,
                token_in: s.fast_swap_sim.token_in.symbol.clone(),
                token_out: s.fast_swap_sim.token_out.symbol.clone(),
                amount_in: s.fast_swap_sim.amount_in.to_string(),
                amount_out: s.fast_swap_sim.amount_out.to_string(),
                gas_cost: s.fast_swap_sim.gas_cost.to_string(),
            },
            slow_prices_a_b,
            slow_prices_a_usdc: s.slow_prices_a_usdc.map(SpotPriceResponse::from),
            slow_prices_b_usdc: s.slow_prices_b_usdc.map(SpotPriceResponse::from),
            slow_prices_eth_usdc: s.slow_prices_eth_usdc.map(SpotPriceResponse::from),
            expected_profit: ExpectedProfitResponse::from(s.expected_profit),
            max_slippage_bps: s.max_slippage_bps,
            congestion_risk_discount_bps: s.congestion_risk_discount_bps,
            fast_prices_a_b_created_at: fast_prices_a_b_created_at.map(|dt| dt.to_rfc3339()),
        }
    }
}

#[derive(Deserialize)]
pub struct SignalQuery {
    pub pair: String,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

pub async fn get_signals_by_pair(
    State(state): State<AppState>,
    Query(params): Query<SignalQuery>,
) -> Result<Json<PaginatedResponse<CrossChainSingleHopResponse>>, Response> {
    let (page, page_size) = params.pagination.sanitize();
    let (offset, limit) = params.pagination.to_offset_limit();

    info!(
        pair = %params.pair,
        page = %page,
        page_size = %page_size,
        "Fetching arbitrage signals"
    );

    let signal_repo = state.db.signal_repository();
    let spot_price_repo = state.db.spot_price_repository();

    let (token_a_symbol, token_b_symbol) = match parse_pair(&params.pair.to_uppercase()) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!("Failed to parse pair: {}", e);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Invalid pair format",
                    "message": format!("Failed to parse pair '{}': {}", params.pair, e)
                })),
            )
                .into_response());
        }
    };

    let (count_result, data_result) = tokio::join!(
        signal_repo.count_by_symbols(&token_a_symbol, &token_b_symbol),
        signal_repo.get_by_symbols(
            &token_a_symbol,
            &token_b_symbol,
            limit,
            offset,
            &spot_price_repo,
        )
    );

    match (count_result, data_result) {
        (Ok(total_count), Ok(signals)) => Ok(Json(PaginatedResponse::new(
            signals
                .into_iter()
                .map(|(id, signal, slow_ts, fast_ts)| CrossChainSingleHopResponse::new(id, signal, slow_ts, fast_ts))
                .collect(),
            page,
            page_size,
            Some(total_count),
        ))),
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!("Failed to fetch arbitrage signals: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error",
                    "message": "Failed to fetch arbitrage signals"
                })),
            )
                .into_response())
        }
    }
}

pub async fn get_signal_by_id(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Json<CrossChainSingleHopResponse>, Response> {
    let signal_repo = state.db.signal_repository();
    let spot_price_repo = state.db.spot_price_repository();

    match signal_repo.get_by_id(id, &spot_price_repo).await {
        Ok(Some((signal, slow_ts, fast_ts))) => Ok(Json(CrossChainSingleHopResponse::new(id, signal, slow_ts, fast_ts))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Not found",
                "message": format!("Signal {} not found", id)
            })),
        )
            .into_response()),
        Err(e) => {
            tracing::error!("Failed to fetch signal {}: {}", id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error",
                    "message": "Failed to fetch signal"
                })),
            )
                .into_response())
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_signals_by_pair))
        .route("/:id", get(get_signal_by_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_query_deserialization() {
        let query = "pair=PEPE-WETH&page=3&page_size=15";
        let parsed: SignalQuery = serde_urlencoded::from_str(query).unwrap();

        assert_eq!(parsed.pair, "PEPE-WETH".to_string());
        assert_eq!(parsed.pagination.page, Some(3));
        assert_eq!(parsed.pagination.page_size, Some(15));
    }

    #[test]
    fn test_pair_filtering_logic() {
        let pair = "PEPE-WETH";
        let parts: Vec<&str> = pair.split('-').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "PEPE");
        assert_eq!(parts[1], "WETH");

        let invalid_pair = "PEPE";
        let invalid_parts: Vec<&str> = invalid_pair.split('-').collect();
        assert_eq!(invalid_parts.len(), 1);

        let invalid_pair2 = "PEPE-WETH-USDC";
        let invalid_parts2: Vec<&str> = invalid_pair2.split('-').collect();
        assert_eq!(invalid_parts2.len(), 3);
    }

    #[test]
    fn test_pair_normalization() {
        let lowercase_pair = "pepe-weth";
        assert_eq!(lowercase_pair.to_uppercase(), "PEPE-WETH");

        let mixed_case_pair = "Pepe-WeTh";
        assert_eq!(mixed_case_pair.to_uppercase(), "PEPE-WETH");

        let already_uppercase = "PEPE-WETH";
        assert_eq!(already_uppercase.to_uppercase(), "PEPE-WETH");
    }
}
