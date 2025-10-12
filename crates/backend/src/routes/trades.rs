use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    models::{PaginatedResponse, PaginationQuery},
    pair::parse_pair,
    AppState,
};

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
    pub expected_profit_a: String,
    pub expected_profit_b: String,
    pub max_slippage_bps: u64,
    pub congestion_risk_discount_bps: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SuccessfulTradeResponse {
    #[serde(flatten)]
    pub base: BaseTradeResponse,
    pub slow_tx_hash: String,
    pub fast_tx_hash: String,
    pub realized_profit_str: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FailedOnSlowTradeResponse {
    #[serde(flatten)]
    pub base: BaseTradeResponse,
    pub slow_tx_hash: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FailedOnFastTradeResponse {
    #[serde(flatten)]
    pub base: BaseTradeResponse,
    pub slow_tx_hash: String,
    pub fast_tx_hash: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "trade_type", content = "trade_data")]
pub enum TradeResultResponse {
    Successful(SuccessfulTradeResponse),
    FailedOnSlow(FailedOnSlowTradeResponse),
    FailedOnFast(FailedOnFastTradeResponse),
}

#[derive(Deserialize)]
pub struct TradeResultQuery {
    pub pair: String,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

// Helper function to fetch signal and build base response
async fn get_base_trade_response(
    signal_id: i64,
    signal_repo: &kuma_core::database::SignalRepository,
) -> Result<BaseTradeResponse, Response> {
    println!("Fetching signal for trade ID: {}", signal_id);
    let signal = signal_repo.get_by_id(signal_id).await.map_err(|e| {
        tracing::error!("Failed to fetch signal for trade {}: {}", signal_id, e);
        (StatusCode::INTERNAL_SERVER_ERROR,
         Json(serde_json::json!({ "error": "Database error", "message": format!("Failed to fetch signal for trade {}", signal_id) }))).into_response()
    })?;
    let signal = signal.ok_or_else(|| {
        tracing::error!("Signal not found for trade {}", signal_id);
        (StatusCode::INTERNAL_SERVER_ERROR,
         Json(serde_json::json!({ "error": "Data inconsistency", "message": format!("Signal {} not found", signal_id) }))).into_response()
    })?;

    Ok(BaseTradeResponse {
        signal_id,
        slow_chain: signal.slow_chain.name.to_string(),
        slow_height: signal.slow_height,
        slow_pool_id: signal.slow_pool_id.to_string(),
        fast_chain: signal.fast_chain.name.to_string(),
        fast_height: signal.fast_height,
        fast_pool_id: signal.fast_pool_id.to_string(),
        slow_swap_token_in_symbol: signal.slow_swap_sim.token_in.symbol,
        slow_swap_token_out_symbol: signal.slow_swap_sim.token_out.symbol,
        slow_swap_amount_in: signal.slow_swap_sim.amount_in.to_string(),
        slow_swap_amount_out: signal.slow_swap_sim.amount_out.to_string(),
        slow_swap_gas_cost: signal.slow_swap_sim.gas_cost.to_string(),
        fast_swap_token_in_symbol: signal.fast_swap_sim.token_in.symbol,
        fast_swap_token_out_symbol: signal.fast_swap_sim.token_out.symbol,
        fast_swap_amount_in: signal.fast_swap_sim.amount_in.to_string(),
        fast_swap_amount_out: signal.fast_swap_sim.amount_out.to_string(),
        fast_swap_gas_cost: signal.fast_swap_sim.gas_cost.to_string(),
        surplus_a: signal.surplus.0.to_string(),
        surplus_b: signal.surplus.1.to_string(),
        expected_profit_a: signal.expected_profit.0.to_string(),
        expected_profit_b: signal.expected_profit.1.to_string(),
        max_slippage_bps: signal.max_slippage_bps,
        congestion_risk_discount_bps: signal.congestion_risk_discount_bps,
    })
}

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
    let signal_repo = state.db.signal_repository();

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

    // Get total count and data for all trade types
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

    println!("Total trade count: {}", total_count);
    let mut all_trade_responses: Vec<TradeResultResponse> = Vec::new();

    if let Ok(successful_trades) = successful_data_res {
        for row in successful_trades {
            let base = get_base_trade_response(row.signal_id, &signal_repo).await?;
            all_trade_responses.push(TradeResultResponse::Successful(SuccessfulTradeResponse {
                base,
                slow_tx_hash: row.slow_tx_hash,
                fast_tx_hash: row.fast_tx_hash,
                realized_profit_str: row.realized_profit_str,
            }));
        }
    }

    if let Ok(failed_slow_trades) = failed_slow_data_res {
        for row in failed_slow_trades {
            let base = get_base_trade_response(row.signal_id, &signal_repo).await?;
            all_trade_responses.push(TradeResultResponse::FailedOnSlow(
                FailedOnSlowTradeResponse {
                    base,
                    slow_tx_hash: row.slow_tx_hash,
                },
            ));
        }
    }

    if let Ok(failed_fast_trades) = failed_fast_data_res {
        for row in failed_fast_trades {
            let base = get_base_trade_response(row.signal_id, &signal_repo).await?;
            all_trade_responses.push(TradeResultResponse::FailedOnFast(
                FailedOnFastTradeResponse {
                    base,
                    slow_tx_hash: row.slow_tx_hash,
                    fast_tx_hash: row.fast_tx_hash,
                },
            ));
        }
    }

    // TODO: Sort all_trade_responses by created_at DESC from signals

    Ok(Json(PaginatedResponse::new(
        all_trade_responses,
        page,
        page_size,
        Some(total_count),
    )))
}

// Specific handlers for each trade type
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
    let signal_repo = state.db.signal_repository();

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
        (Ok(total_count), Ok(trade_rows)) => {
            let mut responses: Vec<SuccessfulTradeResponse> = Vec::with_capacity(trade_rows.len());
            for row in trade_rows {
                let base = get_base_trade_response(row.signal_id, &signal_repo).await?;
                responses.push(SuccessfulTradeResponse {
                    base,
                    slow_tx_hash: row.slow_tx_hash,
                    fast_tx_hash: row.fast_tx_hash,
                    realized_profit_str: row.realized_profit_str,
                });
            }
            Ok(Json(PaginatedResponse::new(
                responses,
                page,
                page_size,
                Some(total_count),
            )))
        }
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
    let signal_repo = state.db.signal_repository();

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
        (Ok(total_count), Ok(trade_rows)) => {
            let mut responses: Vec<FailedOnSlowTradeResponse> =
                Vec::with_capacity(trade_rows.len());
            for row in trade_rows {
                let base = get_base_trade_response(row.signal_id, &signal_repo).await?;
                responses.push(FailedOnSlowTradeResponse {
                    base,
                    slow_tx_hash: row.slow_tx_hash,
                });
            }
            Ok(Json(PaginatedResponse::new(
                responses,
                page,
                page_size,
                Some(total_count),
            )))
        }
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
    let signal_repo = state.db.signal_repository();

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
        (Ok(total_count), Ok(trade_rows)) => {
            let mut responses: Vec<FailedOnFastTradeResponse> =
                Vec::with_capacity(trade_rows.len());
            for row in trade_rows {
                let base = get_base_trade_response(row.signal_id, &signal_repo).await?;
                responses.push(FailedOnFastTradeResponse {
                    base,
                    slow_tx_hash: row.slow_tx_hash,
                    fast_tx_hash: row.fast_tx_hash,
                });
            }
            Ok(Json(PaginatedResponse::new(
                responses,
                page,
                page_size,
                Some(total_count),
            )))
        }
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

#[cfg(test)]
mod tests {
    // TODO: Add tests
}
