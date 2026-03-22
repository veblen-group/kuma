use axum::{
    extract::{Query, State},
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
use tracing::info;

use crate::{
    models::{PaginatedResponse, PaginationQuery},
    pair::parse_pair,
    routes::signals::CrossChainSingleHopResponse,
    AppState,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SuccessfulTradeResponse {
    // TODO: add trade id
    pub signal: CrossChainSingleHopResponse,
    pub slow_tx_hash: String,
    pub fast_tx_hash: String,
    pub realized_profit_str: String,
}

impl SuccessfulTradeResponse {
    fn from_row_and_signal(row: TradeSuccessRow, signal: signals::CrossChainSingleHop) -> Self {
        Self {
            signal: CrossChainSingleHopResponse::from(signal),
            slow_tx_hash: row.slow_tx_hash,
            fast_tx_hash: row.fast_tx_hash,
            realized_profit_str: row.realized_profit_str,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FailedOnSlowTradeResponse {
    // TODO: add trade id
    pub signal: CrossChainSingleHopResponse,
    pub slow_tx_hash: Option<String>,
}

impl FailedOnSlowTradeResponse {
    fn from_row_and_signal(
        row: TradeFailedOnSlowRow,
        signal: signals::CrossChainSingleHop,
    ) -> Self {
        Self {
            signal: CrossChainSingleHopResponse::from(signal),
            slow_tx_hash: row.slow_tx_hash,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FailedOnFastTradeResponse {
    // TODO: add trade id
    pub signal: CrossChainSingleHopResponse,
    pub slow_tx_hash: String,
    pub fast_tx_hash: Option<String>,
}

impl FailedOnFastTradeResponse {
    fn from_row_and_signal(
        row: TradeFailedOnFastRow,
        signal: signals::CrossChainSingleHop,
    ) -> Self {
        Self {
            signal: CrossChainSingleHopResponse::from(signal),
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
) -> color_eyre::eyre::Result<Vec<(Row, CrossChainSingleHop)>>
where
    Row: Send + 'static,
{
    futures::future::try_join_all(rows.into_iter().map(|row| {
        let signal_id = get_id(&row);
        let signal_repo = signal_repo.clone();
        let spot_price_repo = spot_price_repo.clone();
        async move {
            let signal = signal_repo
                .get_by_id(signal_id, &spot_price_repo)
                .await
                .wrap_err("failed to get signal from db for trade")?
                .ok_or_eyre("no signal found in db for trade")?;
            Ok((row, signal))
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
            responses.extend(enriched.into_iter().map(|(row, signal)| {
                TradeResultResponse::Successful(SuccessfulTradeResponse::from_row_and_signal(
                    row, signal,
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
            responses.extend(enriched.into_iter().map(|(row, signal)| {
                TradeResultResponse::FailedOnSlow(FailedOnSlowTradeResponse::from_row_and_signal(
                    row, signal,
                ))
            }));
        }
    }
    if let Ok(rows) = failed_on_fast {
        if let Ok(enriched) = enrich_rows(rows, signal_repo, spot_price_repo, |r| r.signal_id).await
        {
            responses.extend(enriched.into_iter().map(|(row, signal)| {
                TradeResultResponse::FailedOnFast(FailedOnFastTradeResponse::from_row_and_signal(
                    row, signal,
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
    .map(|(row, signal)| SuccessfulTradeResponse::from_row_and_signal(row, signal))
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
    .map(|(row, signal)| FailedOnSlowTradeResponse::from_row_and_signal(row, signal))
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
    .map(|(row, signal)| FailedOnFastTradeResponse::from_row_and_signal(row, signal))
    .collect::<Vec<_>>();

    Ok(Json(PaginatedResponse::new(
        responses,
        page,
        page_size,
        Some(total_count),
    )))
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
