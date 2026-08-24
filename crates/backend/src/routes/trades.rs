//! HTTP handlers for trade result endpoints.
//!
//! Exposes four routes (all under `/trades`):
//! - `GET /` — all outcomes merged, tagged with `trade_type`
//! - `GET /successful`
//! - `GET /failed-on-slow`
//! - `GET /failed-on-fast`
//!
//! Each trade row from the DB contains only a `signal_id` FK — the full signal
//! is fetched via `SignalRepository::get_by_id` in `enrich_rows`, which
//! fans out concurrent fetches for a page of trade rows. Response types
//! (`SuccessfulTradeResponse`, `FailedOnSlowTradeResponse`,
//! `FailedOnFastTradeResponse`) embed the full `CrossChainSingleHopResponse`
//! alongside trade-specific fields (tx hashes, realized profit).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use color_eyre::eyre::{Context as _, OptionExt as _};
use kuma_core::{
    database::{
        SignalRepository, SpotPriceRepository, TradeFailedOnFastRow, TradeFailedOnSlowRow,
        TradeSuccessRow,
    },
    signals::{self, CrossChainSingleHop},
};
use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, Utc};
use tracing::info;

use crate::{
    models::{PaginatedResponse, PaginationQuery},
    pair::parse_pair,
    routes::signals::CrossChainSingleHopResponse,
    AppState,
};

/// Per-token and gas breakdown of realized profit. Populated for trades recorded
/// after this field was added; older rows will have `None` here.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RealizedProfitDetail {
    pub surplus_a: String,
    pub surplus_b: String,
    pub usdc_amount_a: String,
    pub usdc_amount_b: String,
    pub gas_amount_eth: String,
    pub gas_amount_usdc: String,
    pub token_usdc_price_a: f64,
    pub token_usdc_price_b: f64,
    pub gas_price_usdc: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SuccessfulTradeResponse {
    pub id: i64,
    pub signal: CrossChainSingleHopResponse,
    pub slow_tx_hash: String,
    pub fast_tx_hash: String,
    pub realized_profit_str: String,
    pub realized_profit_detail: Option<RealizedProfitDetail>,
}

impl SuccessfulTradeResponse {
    fn from_row_and_signal(row: TradeSuccessRow, signal: signals::CrossChainSingleHop, slow_ts: Option<DateTime<Utc>>, fast_ts: Option<DateTime<Utc>>) -> Self {
        let realized_profit_detail = match (
            row.realized_surplus_a,
            row.realized_surplus_b,
            row.realized_usdc_amount_a,
            row.realized_usdc_amount_b,
            row.realized_gas_amount_eth,
            row.realized_gas_amount_usdc,
            row.realized_token_usdc_price_a,
            row.realized_token_usdc_price_b,
            row.realized_gas_price_usdc,
        ) {
            (
                Some(surplus_a),
                Some(surplus_b),
                Some(usdc_amount_a),
                Some(usdc_amount_b),
                Some(gas_amount_eth),
                Some(gas_amount_usdc),
                Some(token_usdc_price_a),
                Some(token_usdc_price_b),
                Some(gas_price_usdc),
            ) => Some(RealizedProfitDetail {
                surplus_a,
                surplus_b,
                usdc_amount_a,
                usdc_amount_b,
                gas_amount_eth,
                gas_amount_usdc,
                token_usdc_price_a,
                token_usdc_price_b,
                gas_price_usdc,
            }),
            _ => None,
        };

        Self {
            id: row.id,
            signal: CrossChainSingleHopResponse::new(row.signal_id, signal, slow_ts, fast_ts),
            slow_tx_hash: row.slow_tx_hash,
            fast_tx_hash: row.fast_tx_hash,
            realized_profit_str: row.realized_profit_str,
            realized_profit_detail,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FailedOnSlowTradeResponse {
    pub id: i64,
    pub signal: CrossChainSingleHopResponse,
    pub slow_tx_hash: Option<String>,
}

impl FailedOnSlowTradeResponse {
    fn from_row_and_signal(
        row: TradeFailedOnSlowRow,
        signal: signals::CrossChainSingleHop,
        slow_ts: Option<DateTime<Utc>>,
        fast_ts: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id: row.id,
            signal: CrossChainSingleHopResponse::new(row.signal_id, signal, slow_ts, fast_ts),
            slow_tx_hash: row.slow_tx_hash,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FailedOnFastTradeResponse {
    pub id: i64,
    pub signal: CrossChainSingleHopResponse,
    pub slow_tx_hash: String,
    pub fast_tx_hash: Option<String>,
}

impl FailedOnFastTradeResponse {
    fn from_row_and_signal(
        row: TradeFailedOnFastRow,
        signal: signals::CrossChainSingleHop,
        slow_ts: Option<DateTime<Utc>>,
        fast_ts: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id: row.id,
            signal: CrossChainSingleHopResponse::new(row.signal_id, signal, slow_ts, fast_ts),
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

#[derive(Deserialize)]
pub struct TradeResultQuery {
    pub pair: String,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[allow(clippy::result_large_err)]
fn parse_pair_or_err(pair: &str) -> Result<(String, String), Response> {
    parse_pair(&pair.to_uppercase()).map_err(|e| {
        tracing::error!("Failed to parse pair: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Invalid pair format",
                "message": format!("Failed to parse pair '{}': {}", pair, e)
            })),
        )
            .into_response()
    })
}

async fn enrich_rows<Row>(
    rows: Vec<Row>,
    signal_repo: SignalRepository,
    spot_price_repo: SpotPriceRepository,
    get_id: impl Fn(&Row) -> i64,
) -> color_eyre::eyre::Result<Vec<(Row, CrossChainSingleHop, Option<DateTime<Utc>>, Option<DateTime<Utc>>)>>
where
    Row: Send + 'static,
{
    futures::future::try_join_all(rows.into_iter().map(|row| {
        let signal_id = get_id(&row);
        let signal_repo = signal_repo.clone();
        let spot_price_repo = spot_price_repo.clone();
        async move {
            let (signal, slow_ts, fast_ts) = signal_repo
                .get_by_id(signal_id, &spot_price_repo)
                .await
                .wrap_err("failed to get signal from db for trade")?
                .ok_or_eyre("no signal found in db for trade")?;
            Ok((row, signal, slow_ts, fast_ts))
        }
    }))
    .await
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
    let (token_a, token_b) = parse_pair_or_err(&params.pair)?;

    let (
        success,
        success_count,
        failed_on_slow,
        failed_on_slow_count,
        failed_on_fast,
        failed_on_fast_count,
    ) = tokio::join!(
        trade_repo.get_successful_by_symbols(&token_a, &token_b, limit, offset),
        trade_repo.count_successful_by_symbols(&token_a, &token_b),
        trade_repo.get_failed_on_slow_by_symbols(&token_a, &token_b, limit, offset),
        trade_repo.count_failed_on_slow_by_symbols(&token_a, &token_b),
        trade_repo.get_failed_on_fast_by_symbols(&token_a, &token_b, limit, offset),
        trade_repo.count_failed_on_fast_by_symbols(&token_a, &token_b)
    );

    let signal_repo = state.db.signal_repository();
    let spot_price_repo = state.db.spot_price_repository();

    let mut responses: Vec<TradeResultResponse> = Vec::new();

    if let Ok(rows) = success {
        if let Ok(enriched) = enrich_rows(rows, signal_repo.clone(), spot_price_repo.clone(), |r| {
            r.signal_id
        })
        .await
        {
            responses.extend(enriched.into_iter().map(|(row, signal, slow_ts, fast_ts)| {
                TradeResultResponse::Successful(SuccessfulTradeResponse::from_row_and_signal(
                    row, signal, slow_ts, fast_ts,
                ))
            }));
        }
    }
    if let Ok(rows) = failed_on_slow {
        if let Ok(enriched) = enrich_rows(rows, signal_repo.clone(), spot_price_repo.clone(), |r| {
            r.signal_id
        })
        .await
        {
            responses.extend(enriched.into_iter().map(|(row, signal, slow_ts, fast_ts)| {
                TradeResultResponse::FailedOnSlow(FailedOnSlowTradeResponse::from_row_and_signal(
                    row, signal, slow_ts, fast_ts,
                ))
            }));
        }
    }
    if let Ok(rows) = failed_on_fast {
        if let Ok(enriched) = enrich_rows(rows, signal_repo, spot_price_repo, |r| r.signal_id).await
        {
            responses.extend(enriched.into_iter().map(|(row, signal, slow_ts, fast_ts)| {
                TradeResultResponse::FailedOnFast(FailedOnFastTradeResponse::from_row_and_signal(
                    row, signal, slow_ts, fast_ts,
                ))
            }));
        }
    }

    let total_count = match (success_count, failed_on_slow_count, failed_on_fast_count) {
        (Ok(s), Ok(slow), Ok(fast)) => s + slow + fast,
        (s, slow, fast) => {
            tracing::error!(
                success_count_err = ?s.err(),
                failed_on_slow_count_err = ?slow.err(),
                failed_on_fast_count_err = ?fast.err(),
                "Failed to fetch total trade counts"
            );
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error",
                    "message": "Failed to fetch total trade counts"
                })),
            )
                .into_response());
        }
    };

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
    let (token_a, token_b) = parse_pair_or_err(&params.pair)?;

    let (count_result, data_result) = tokio::join!(
        trade_repo.count_successful_by_symbols(&token_a, &token_b),
        trade_repo.get_successful_by_symbols(&token_a, &token_b, limit, offset)
    );

    let (total_count, rows) = match (count_result, data_result) {
        (Ok(c), Ok(r)) => (c, r),
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!("Failed to fetch successful trade results: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error",
                    "message": "Failed to fetch successful trade results"
                })),
            )
                .into_response());
        }
    };
    let responses = enrich_rows(
        rows,
        state.db.signal_repository(),
        state.db.spot_price_repository(),
        |r| r.signal_id,
    )
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Database error",
                "message": "Failed to fetch successful trade results"
            })),
        )
            .into_response()
    })?
    .into_iter()
    .map(|(row, signal, slow_ts, fast_ts)| SuccessfulTradeResponse::from_row_and_signal(row, signal, slow_ts, fast_ts))
    .collect::<Vec<_>>();

    Ok(Json(PaginatedResponse::new(
        responses,
        page,
        page_size,
        Some(total_count),
    )))
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
    let (token_a, token_b) = parse_pair_or_err(&params.pair)?;

    let (count_result, data_result) = tokio::join!(
        trade_repo.count_failed_on_slow_by_symbols(&token_a, &token_b),
        trade_repo.get_failed_on_slow_by_symbols(&token_a, &token_b, limit, offset)
    );

    let (total_count, rows) = match (count_result, data_result) {
        (Ok(c), Ok(r)) => (c, r),
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!("Failed to fetch failed on slow trade results: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error",
                    "message": "Failed to fetch failed on slow trade results"
                })),
            )
                .into_response());
        }
    };

    let responses = enrich_rows(
        rows,
        state.db.signal_repository(),
        state.db.spot_price_repository(),
        |r| r.signal_id,
    )
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Database error",
                "message": "Failed to fetch failed on slow trade results"
            })),
        )
            .into_response()
    })?
    .into_iter()
    .map(|(row, signal, slow_ts, fast_ts)| FailedOnSlowTradeResponse::from_row_and_signal(row, signal, slow_ts, fast_ts))
    .collect::<Vec<_>>();

    Ok(Json(PaginatedResponse::new(
        responses,
        page,
        page_size,
        Some(total_count),
    )))
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
    let (token_a, token_b) = parse_pair_or_err(&params.pair)?;

    let (count_result, data_result) = tokio::join!(
        trade_repo.count_failed_on_fast_by_symbols(&token_a, &token_b),
        trade_repo.get_failed_on_fast_by_symbols(&token_a, &token_b, limit, offset)
    );

    let (total_count, rows) = match (count_result, data_result) {
        (Ok(c), Ok(r)) => (c, r),
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!("Failed to fetch failed on fast trade results: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error",
                    "message": "Failed to fetch failed on fast trade results"
                })),
            )
                .into_response());
        }
    };

    let responses = enrich_rows(
        rows,
        state.db.signal_repository(),
        state.db.spot_price_repository(),
        |r| r.signal_id,
    )
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Database error",
                "message": "Failed to fetch failed on fast trade results"
            })),
        )
            .into_response()
    })?
    .into_iter()
    .map(|(row, signal, slow_ts, fast_ts)| FailedOnFastTradeResponse::from_row_and_signal(row, signal, slow_ts, fast_ts))
    .collect::<Vec<_>>();

    Ok(Json(PaginatedResponse::new(
        responses,
        page,
        page_size,
        Some(total_count),
    )))
}

/// Slim trade result for the signal detail page — omits the embedded signal since
/// the caller already has it. Uses internally-tagged serde so `trade_type` is a
/// top-level field alongside the trade-specific fields.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "trade_type")]
pub enum SignalTradeResultResponse {
    Successful {
        id: i64,
        slow_tx_hash: String,
        fast_tx_hash: String,
        realized_profit_str: String,
    },
    FailedOnSlow {
        id: i64,
        slow_tx_hash: Option<String>,
    },
    FailedOnFast {
        id: i64,
        slow_tx_hash: String,
        fast_tx_hash: Option<String>,
    },
}

/// `GET /trades/by-signal/:signal_id` — return the trade result(s) for a specific signal.
///
/// There should be at most one trade per signal in practice, but the response is a
/// `Vec` to be forward-compatible if that assumption changes. Returns an empty list
/// when no trade has been recorded for the signal.
pub async fn get_trades_by_signal(
    Path(signal_id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Json<Vec<SignalTradeResultResponse>>, Response> {
    let trade_repo = state.db.trade_repository();

    let (success, failed_slow, failed_fast) = tokio::join!(
        trade_repo.get_successful_by_signal_id(signal_id),
        trade_repo.get_failed_on_slow_by_signal_id(signal_id),
        trade_repo.get_failed_on_fast_by_signal_id(signal_id),
    );

    let mut results: Vec<SignalTradeResultResponse> = Vec::new();

    match success {
        Ok(Some(row)) => results.push(SignalTradeResultResponse::Successful {
            id: row.id,
            slow_tx_hash: row.slow_tx_hash,
            fast_tx_hash: row.fast_tx_hash,
            realized_profit_str: row.realized_profit_str,
        }),
        Ok(None) => {}
        Err(e) => {
            tracing::error!("Failed to fetch successful trade for signal {}: {}", signal_id, e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            ).into_response());
        }
    }

    match failed_slow {
        Ok(Some(row)) => results.push(SignalTradeResultResponse::FailedOnSlow {
            id: row.id,
            slow_tx_hash: row.slow_tx_hash,
        }),
        Ok(None) => {}
        Err(e) => {
            tracing::error!("Failed to fetch failed-on-slow trade for signal {}: {}", signal_id, e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            ).into_response());
        }
    }

    match failed_fast {
        Ok(Some(row)) => results.push(SignalTradeResultResponse::FailedOnFast {
            id: row.id,
            slow_tx_hash: row.slow_tx_hash,
            fast_tx_hash: row.fast_tx_hash,
        }),
        Ok(None) => {}
        Err(e) => {
            tracing::error!("Failed to fetch failed-on-fast trade for signal {}: {}", signal_id, e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            ).into_response());
        }
    }

    Ok(Json(results))
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
        .route("/by-signal/:signal_id", get(get_trades_by_signal))
}
