use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use kuma_core::spot_prices::SpotPrices;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    models::{PaginatedResponse, PaginationQuery},
    pair::parse_pair,
    AppState,
};

/// API response type for spot prices. Exposes only the chain name — never the
/// full `Chain` struct which contains API keys and RPC credentials.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SpotPriceResponse {
    // TODO: add id, created at
    pub chain: String,
    pub pair_token_a: String,
    pub pair_token_b: String,
    pub block_height: u64,
    pub min_price: f64,
    pub max_price: f64,
    pub min_pool_id: String,
    pub max_pool_id: String,
}

impl From<SpotPrices> for SpotPriceResponse {
    fn from(sp: SpotPrices) -> Self {
        Self {
            chain: sp.chain.name.to_string(),
            pair_token_a: sp.pair.token_a().symbol.clone(),
            pair_token_b: sp.pair.token_b().symbol.clone(),
            block_height: sp.block_height,
            min_price: sp.min_price,
            max_price: sp.max_price,
            min_pool_id: sp.min_pool_id.to_string(),
            max_pool_id: sp.max_pool_id.to_string(),
        }
    }
}

#[derive(Deserialize)]
pub struct SpotPriceByPairQuery {
    pub pair: String,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

pub async fn get_spot_prices_by_pair(
    State(state): State<AppState>,
    Query(params): Query<SpotPriceByPairQuery>,
) -> Result<Json<PaginatedResponse<SpotPriceResponse>>, Response> {
    let (page, page_size) = params.pagination.sanitize();
    let (offset, limit) = params.pagination.to_offset_limit();

    info!(
        pair = ?params.pair,
        page = %page,
        page_size = %page_size,
        "Fetching spot prices"
    );

    let repo = state.db.spot_price_repository();

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
        repo.count_by_symbols(&token_a_symbol, &token_b_symbol),
        repo.get_by_symbols(&token_a_symbol, &token_b_symbol, limit, offset)
    );

    match (count_result, data_result) {
        (Ok(total_count), Ok(prices)) => Ok(Json(PaginatedResponse::new(
            prices.into_iter().map(SpotPriceResponse::from).collect(),
            page,
            page_size,
            Some(total_count),
        ))),
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!("Failed to fetch spot prices: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error",
                    "message": "Failed to fetch spot prices"
                })),
            )
                .into_response())
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(get_spot_prices_by_pair))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spot_price_query_deserialization() {
        let query = "pair=WETH-USDC&page=2&page_size=50";
        let parsed: SpotPriceByPairQuery = serde_urlencoded::from_str(query).unwrap();

        assert_eq!(parsed.pair, "WETH-USDC".to_string());
        assert_eq!(parsed.pagination.page, Some(2));
        assert_eq!(parsed.pagination.page_size, Some(50));
    }

    #[test]
    fn test_pagination_sanitization() {
        use crate::models::PaginationQuery;

        let pagination = PaginationQuery {
            page: None,
            page_size: None,
        };
        let (page, page_size) = pagination.sanitize();
        assert_eq!(page, 1);
        assert_eq!(page_size, 20);

        let pagination = PaginationQuery {
            page: Some(2),
            page_size: Some(200),
        };
        let (page, page_size) = pagination.sanitize();
        assert_eq!(page, 2);
        assert_eq!(page_size, 100);

        let pagination = PaginationQuery {
            page: Some(0),
            page_size: Some(0),
        };
        let (page, page_size) = pagination.sanitize();
        assert_eq!(page, 1);
        assert_eq!(page_size, 1);
    }
}
