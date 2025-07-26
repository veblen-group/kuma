use crate::{
    models::{ArbitrageSignal, PaginationQuery, PaginatedResponse},
    state::AppState,
};
use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tracing::info;

#[derive(Deserialize)]
pub struct SignalQuery {
    pub block_height: u64,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

pub async fn get_signals(
    State(state): State<AppState>,
    Query(params): Query<SignalQuery>,
) -> Json<PaginatedResponse<ArbitrageSignal>> {
    let (page, page_size) = params.pagination.sanitize();
    let (offset, limit) = params.pagination.to_offset_limit();
    
    info!(
        block_height = %params.block_height, 
        page = %page, 
        page_size = %page_size,
        "Fetching arbitrage signals"
    );
    
    let repo = state.db.arbitrage_signal_repository();
    
    // Get total count and data in parallel
    let (count_result, data_result) = tokio::join!(
        repo.count_by_block_height(params.block_height),
        repo.get_by_block_height(params.block_height, limit, offset)
    );
    
    match (count_result, data_result) {
        (Ok(total_count), Ok(signals)) => {
            Json(PaginatedResponse::new(signals, page, page_size, Some(total_count)))
        }
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!("Failed to fetch arbitrage signals: {}", e);
            Json(PaginatedResponse::new(vec![], page, page_size, Some(0)))
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(get_signals))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_query_deserialization() {
        let query = "block_height=19500000&page=3&page_size=15";
        let parsed: SignalQuery = serde_urlencoded::from_str(query).unwrap();
        
        assert_eq!(parsed.block_height, 19500000);
        assert_eq!(parsed.pagination.page, Some(3));
        assert_eq!(parsed.pagination.page_size, Some(15));
    }

    #[test]
    fn test_signal_query_defaults() {
        let query = "block_height=19500000";
        let parsed: SignalQuery = serde_urlencoded::from_str(query).unwrap();
        
        assert_eq!(parsed.block_height, 19500000);
        assert_eq!(parsed.pagination.page, None);
        assert_eq!(parsed.pagination.page_size, None);
    }
}
