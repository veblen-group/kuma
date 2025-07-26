use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tracing::info;

use crate::{
    models::{SpotPrice, PaginationQuery, PaginatedResponse},
    state::AppState,
};

#[derive(Deserialize)]
pub struct SpotPriceQuery {
    pub block_height: u64,
    pub pair: Option<String>, // Format: "TokenA-TokenB"
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

pub async fn get_spot_prices(
    State(state): State<AppState>,
    Query(params): Query<SpotPriceQuery>,
) -> Json<PaginatedResponse<SpotPrice>> {
    let (page, page_size) = params.pagination.sanitize();
    let (offset, limit) = params.pagination.to_offset_limit();
    
    info!(
        block_height = %params.block_height, 
        pair = ?params.pair, 
        page = %page, 
        page_size = %page_size,
        "Fetching spot prices"
    );

    let repo = state.db.spot_price_repository();
    
    // Get total count and data in parallel
    let (count_result, data_result) = tokio::join!(
        repo.count_by_block_height(params.block_height, params.pair.as_deref()),
        repo.get_by_block_height(params.block_height, params.pair.as_deref(), limit, offset)
    );
    
    match (count_result, data_result) {
        (Ok(total_count), Ok(prices)) => {
            Json(PaginatedResponse::new(prices, page, page_size, Some(total_count)))
        }
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!("Failed to fetch spot prices: {}", e);
            Json(PaginatedResponse::new(vec![], page, page_size, Some(0)))
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(get_spot_prices))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spot_price_query_deserialization() {
        let query = "block_height=19500000&pair=WETH-USDC&page=2&page_size=50";
        let parsed: SpotPriceQuery = serde_urlencoded::from_str(query).unwrap();
        
        assert_eq!(parsed.block_height, 19500000);
        assert_eq!(parsed.pair, Some("WETH-USDC".to_string()));
        assert_eq!(parsed.pagination.page, Some(2));
        assert_eq!(parsed.pagination.page_size, Some(50));
    }

    #[test]
    fn test_spot_price_query_without_pair() {
        let query = "block_height=19500000";
        let parsed: SpotPriceQuery = serde_urlencoded::from_str(query).unwrap();
        
        assert_eq!(parsed.block_height, 19500000);
        assert_eq!(parsed.pair, None);
        assert_eq!(parsed.pagination.page, None);
        assert_eq!(parsed.pagination.page_size, None);
    }

    #[test]
    fn test_pagination_sanitization() {
        use crate::models::PaginationQuery;
        
        // Test defaults
        let pagination = PaginationQuery { page: None, page_size: None };
        let (page, page_size) = pagination.sanitize();
        assert_eq!(page, 1);
        assert_eq!(page_size, 20);
        
        // Test max page size enforcement
        let pagination = PaginationQuery { page: Some(2), page_size: Some(200) };
        let (page, page_size) = pagination.sanitize();
        assert_eq!(page, 2);
        assert_eq!(page_size, 100); // capped at MAX_PAGE_SIZE
        
        // Test minimum values
        let pagination = PaginationQuery { page: Some(0), page_size: Some(0) };
        let (page, page_size) = pagination.sanitize();
        assert_eq!(page, 1);
        assert_eq!(page_size, 1);
    }
}
