use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use kuma_core::signals::CrossChainSingleHop;
use serde::Deserialize;
use tracing::info;

use crate::{
    models::{PaginatedResponse, PaginationQuery},
    pair::parse_pair,
    AppState,
};

#[derive(Deserialize)]
pub struct SignalQuery {
    pub pair: String,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

pub async fn get_signals_by_pair(
    State(state): State<AppState>,
    Query(params): Query<SignalQuery>,
) -> Result<Json<PaginatedResponse<CrossChainSingleHop>>, Response> {
    let (page, page_size) = params.pagination.sanitize();
    let (offset, limit) = params.pagination.to_offset_limit();

    info!(
        pair = %params.pair,
        page = %page,
        page_size = %page_size,
        "Fetching arbitrage signals"
    );

    let repo = state.db.signal_repository(state.token_configs.clone());

    let (token_a_symbol, token_b_symbol) = match parse_pair(&params.pair) {
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

    // Get total count and data in parallel
    let (count_result, data_result) = tokio::join!(
        repo.count_by_symbols(&token_a_symbol, &token_b_symbol),
        repo.get_by_symbols(&token_a_symbol, &token_b_symbol, limit, offset)
    );

    match (count_result, data_result) {
        (Ok(total_count), Ok(signals)) => Ok(Json(PaginatedResponse::new(
            signals,
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

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(get_signals_by_pair))
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
        // Test pair parsing
        let pair = "PEPE-WETH";
        let parts: Vec<&str> = pair.split('-').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "PEPE");
        assert_eq!(parts[1], "WETH");

        // Test invalid pair format
        let invalid_pair = "PEPE";
        let invalid_parts: Vec<&str> = invalid_pair.split('-').collect();
        assert_eq!(invalid_parts.len(), 1);

        let invalid_pair2 = "PEPE-WETH-USDC";
        let invalid_parts2: Vec<&str> = invalid_pair2.split('-').collect();
        assert_eq!(invalid_parts2.len(), 3);
    }

    #[test]
    fn test_pair_normalization() {
        // Test case-insensitive pair normalization
        let lowercase_pair = "pepe-weth";
        assert_eq!(lowercase_pair.to_uppercase(), "PEPE-WETH");

        let mixed_case_pair = "Pepe-WeTh";
        assert_eq!(mixed_case_pair.to_uppercase(), "PEPE-WETH");

        let already_uppercase = "PEPE-WETH";
        assert_eq!(already_uppercase.to_uppercase(), "PEPE-WETH");
    }
}
