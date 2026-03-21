use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use kuma_core::database::{TradeFailedOnFastRow, TradeFailedOnSlowRow, TradeSuccessRow};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    models::{PaginatedResponse, PaginationQuery},
    pair::parse_pair,
    AppState,
};

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Signal fields shared across all trade outcome types.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BaseTradeResponse {
    pub signal_id: i64,
    pub slow_chain: String,
    pub slow_height: u64,
    pub slow_pool_id: String,
    pub fast_chain: String,
    pub fast_height: u64,
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
    pub slow_base_fee: u64,
    pub fast_base_fee: u64,
    pub max_slippage_bps: u64,
    pub congestion_risk_discount_bps: u64,
}

/// Build a `BaseTradeResponse` from the signal columns that are joined into
/// every trade row — avoids a secondary DB lookup per row.
macro_rules! base_from_row {
    ($row:expr, $signal_id:expr) => {
        BaseTradeResponse {
            signal_id: $signal_id,
            slow_chain: $row.slow_chain.clone(),
            slow_height: $row.slow_height as u64,
            slow_pool_id: $row.slow_pool_id.clone(),
            fast_chain: $row.fast_chain.clone(),
            fast_height: $row.fast_height as u64,
            fast_pool_id: $row.fast_pool_id.clone(),
            slow_swap_token_in_symbol: $row.slow_swap_token_in_symbol.clone(),
            slow_swap_token_out_symbol: $row.slow_swap_token_out_symbol.clone(),
            slow_swap_amount_in: $row.slow_swap_amount_in.clone(),
            slow_swap_amount_out: $row.slow_swap_amount_out.clone(),
            slow_swap_gas_cost: $row.slow_swap_gas_cost.clone(),
            fast_swap_token_in_symbol: $row.fast_swap_token_in_symbol.clone(),
            fast_swap_token_out_symbol: $row.fast_swap_token_out_symbol.clone(),
            fast_swap_amount_in: $row.fast_swap_amount_in.clone(),
            fast_swap_amount_out: $row.fast_swap_amount_out.clone(),
            fast_swap_gas_cost: $row.fast_swap_gas_cost.clone(),
            surplus_a: $row.surplus_a.clone(),
            surplus_b: $row.surplus_b.clone(),
            min_token_amount_a: $row.min_token_amount_a.clone(),
            min_token_amount_b: $row.min_token_amount_b.clone(),
            min_usdc_amount_a: $row.min_usdc_amount_a.clone(),
            min_usdc_amount_b: $row.min_usdc_amount_b.clone(),
            min_total_amount_usdc: $row.min_total_amount_usdc.clone(),
            max_slippage_token_amount_a: $row.max_slippage_token_amount_a.clone(),
            max_slippage_token_amount_b: $row.max_slippage_token_amount_b.clone(),
            token_usdc_price_a: $row.token_usdc_price_a,
            token_usdc_price_b: $row.token_usdc_price_b,
            gas_cost_eth_slow: $row.gas_cost_eth_slow.clone(),
            gas_cost_eth_fast: $row.gas_cost_eth_fast.clone(),
            total_gas_cost_eth: $row.total_gas_cost_eth.clone(),
            eth_usdc_price: $row.eth_usdc_price,
            gas_cost_usdc_slow: $row.gas_cost_usdc_slow.clone(),
            gas_cost_usdc_fast: $row.gas_cost_usdc_fast.clone(),
            total_gas_cost_usdc: $row.total_gas_cost_usdc.clone(),
            slow_base_fee: $row.slow_base_fee as u64,
            fast_base_fee: $row.fast_base_fee as u64,
            max_slippage_bps: $row.max_slippage_bps as u64,
            congestion_risk_discount_bps: $row.congestion_risk_discount_bps as u64,
        }
    };
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SuccessfulTradeResponse {
    #[serde(flatten)]
    pub base: BaseTradeResponse,
    pub slow_tx_hash: String,
    pub fast_tx_hash: String,
    pub realized_profit_str: String,
}

impl From<TradeSuccessRow> for SuccessfulTradeResponse {
    fn from(row: TradeSuccessRow) -> Self {
        let base = base_from_row!(row, row.signal_id);
        Self {
            base,
            slow_tx_hash: row.slow_tx_hash,
            fast_tx_hash: row.fast_tx_hash,
            realized_profit_str: row.realized_profit_str,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FailedOnSlowTradeResponse {
    #[serde(flatten)]
    pub base: BaseTradeResponse,
    pub slow_tx_hash: Option<String>,
}

impl From<TradeFailedOnSlowRow> for FailedOnSlowTradeResponse {
    fn from(row: TradeFailedOnSlowRow) -> Self {
        let base = base_from_row!(row, row.signal_id);
        Self {
            base,
            slow_tx_hash: row.slow_tx_hash,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FailedOnFastTradeResponse {
    #[serde(flatten)]
    pub base: BaseTradeResponse,
    pub slow_tx_hash: String,
    pub fast_tx_hash: Option<String>,
}

impl From<TradeFailedOnFastRow> for FailedOnFastTradeResponse {
    fn from(row: TradeFailedOnFastRow) -> Self {
        let base = base_from_row!(row, row.signal_id);
        Self {
            base,
            slow_tx_hash: row.slow_tx_hash,
            fast_tx_hash: row.fast_tx_hash,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "trade_type", content = "trade_data")]
pub enum TradeResultResponse {
    Successful(SuccessfulTradeResponse),
    FailedOnSlow(FailedOnSlowTradeResponse),
    FailedOnFast(FailedOnFastTradeResponse),
}

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TradeResultQuery {
    pub pair: String,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn get_all_trade_results_by_pair(
    State(state): State<AppState>,
    Query(params): Query<TradeResultQuery>,
) -> Result<Json<PaginatedResponse<TradeResultResponse>>, Response> {
    let (page, page_size) = params.pagination.sanitize();
    let (offset, limit) = params.pagination.to_offset_limit();

    info!(
        pair = %params.pair,
        page = %page,
        page_size = %page_size,
        "Fetching all trade results"
    );

    let trade_repo = state.db.trade_repository();

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

    let (
        successful_count_res,
        failed_slow_count_res,
        failed_fast_count_res,
        successful_data_res,
        failed_slow_data_res,
        failed_fast_data_res,
    ) = tokio::join!(
        trade_repo.count_successful_by_symbols(&token_a_symbol, &token_b_symbol),
        trade_repo.count_failed_on_slow_by_symbols(&token_a_symbol, &token_b_symbol),
        trade_repo.count_failed_on_fast_by_symbols(&token_a_symbol, &token_b_symbol),
        trade_repo.get_successful_by_symbols(&token_a_symbol, &token_b_symbol, limit, offset),
        trade_repo.get_failed_on_slow_by_symbols(&token_a_symbol, &token_b_symbol, limit, offset),
        trade_repo.get_failed_on_fast_by_symbols(&token_a_symbol, &token_b_symbol, limit, offset)
    );

    let total_count = successful_count_res.unwrap_or(0)
        + failed_slow_count_res.unwrap_or(0)
        + failed_fast_count_res.unwrap_or(0);

    let mut responses: Vec<TradeResultResponse> = Vec::new();

    if let Ok(rows) = successful_data_res {
        responses.extend(
            rows.into_iter()
                .map(|r| TradeResultResponse::Successful(SuccessfulTradeResponse::from(r))),
        );
    }
    if let Ok(rows) = failed_slow_data_res {
        responses.extend(
            rows.into_iter()
                .map(|r| TradeResultResponse::FailedOnSlow(FailedOnSlowTradeResponse::from(r))),
        );
    }
    if let Ok(rows) = failed_fast_data_res {
        responses.extend(
            rows.into_iter()
                .map(|r| TradeResultResponse::FailedOnFast(FailedOnFastTradeResponse::from(r))),
        );
    }

    // TODO: sort responses by created_at DESC (requires created_at in the joined row)
    Ok(Json(PaginatedResponse::new(
        responses,
        page,
        page_size,
        Some(total_count),
    )))
}

pub async fn get_successful_trade_results_by_pair(
    State(state): State<AppState>,
    Query(params): Query<TradeResultQuery>,
) -> Result<Json<PaginatedResponse<SuccessfulTradeResponse>>, Response> {
    let (page, page_size) = params.pagination.sanitize();
    let (offset, limit) = params.pagination.to_offset_limit();

    info!(
        pair = %params.pair,
        page = %page,
        page_size = %page_size,
        "Fetching successful trade results"
    );

    let trade_repo = state.db.trade_repository();

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
        trade_repo.count_successful_by_symbols(&token_a_symbol, &token_b_symbol),
        trade_repo.get_successful_by_symbols(&token_a_symbol, &token_b_symbol, limit, offset)
    );

    match (count_result, data_result) {
        (Ok(total_count), Ok(rows)) => Ok(Json(PaginatedResponse::new(
            rows.into_iter().map(SuccessfulTradeResponse::from).collect(),
            page,
            page_size,
            Some(total_count),
        ))),
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!("Failed to fetch successful trade results: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error",
                    "message": "Failed to fetch successful trade results"
                })),
            )
                .into_response())
        }
    }
}

pub async fn get_failed_on_slow_trade_results_by_pair(
    State(state): State<AppState>,
    Query(params): Query<TradeResultQuery>,
) -> Result<Json<PaginatedResponse<FailedOnSlowTradeResponse>>, Response> {
    let (page, page_size) = params.pagination.sanitize();
    let (offset, limit) = params.pagination.to_offset_limit();

    info!(
        pair = %params.pair,
        page = %page,
        page_size = %page_size,
        "Fetching failed on slow trade results"
    );

    let trade_repo = state.db.trade_repository();

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
        trade_repo.count_failed_on_slow_by_symbols(&token_a_symbol, &token_b_symbol),
        trade_repo.get_failed_on_slow_by_symbols(&token_a_symbol, &token_b_symbol, limit, offset)
    );

    match (count_result, data_result) {
        (Ok(total_count), Ok(rows)) => Ok(Json(PaginatedResponse::new(
            rows.into_iter()
                .map(FailedOnSlowTradeResponse::from)
                .collect(),
            page,
            page_size,
            Some(total_count),
        ))),
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!("Failed to fetch failed on slow trade results: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error",
                    "message": "Failed to fetch failed on slow trade results"
                })),
            )
                .into_response())
        }
    }
}

pub async fn get_failed_on_fast_trade_results_by_pair(
    State(state): State<AppState>,
    Query(params): Query<TradeResultQuery>,
) -> Result<Json<PaginatedResponse<FailedOnFastTradeResponse>>, Response> {
    let (page, page_size) = params.pagination.sanitize();
    let (offset, limit) = params.pagination.to_offset_limit();

    info!(
        pair = %params.pair,
        page = %page,
        page_size = %page_size,
        "Fetching failed on fast trade results"
    );

    let trade_repo = state.db.trade_repository();

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
        trade_repo.count_failed_on_fast_by_symbols(&token_a_symbol, &token_b_symbol),
        trade_repo.get_failed_on_fast_by_symbols(&token_a_symbol, &token_b_symbol, limit, offset)
    );

    match (count_result, data_result) {
        (Ok(total_count), Ok(rows)) => Ok(Json(PaginatedResponse::new(
            rows.into_iter()
                .map(FailedOnFastTradeResponse::from)
                .collect(),
            page,
            page_size,
            Some(total_count),
        ))),
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!("Failed to fetch failed on fast trade results: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error",
                    "message": "Failed to fetch failed on fast trade results"
                })),
            )
                .into_response())
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_all_trade_results_by_pair))
        .route("/successful", get(get_successful_trade_results_by_pair))
        .route(
            "/failed-on-slow",
            get(get_failed_on_slow_trade_results_by_pair),
        )
        .route(
            "/failed-on-fast",
            get(get_failed_on_fast_trade_results_by_pair),
        )
}
