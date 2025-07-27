use crate::{
    models::{ArbitrageSignal, PaginationQuery, PaginatedResponse},
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
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

#[derive(Deserialize)]
pub struct ChainQuery {
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

pub async fn get_signals_by_chain(
    State(state): State<AppState>,
    Path(chain): Path<String>,
    Query(params): Query<ChainQuery>,
) -> Json<PaginatedResponse<ArbitrageSignal>> {
    let (page, page_size) = params.pagination.sanitize();
    let (offset, limit) = params.pagination.to_offset_limit();
    
    info!(
        chain = %chain,
        page = %page, 
        page_size = %page_size,
        "Fetching arbitrage signals by chain"
    );
    
    let repo = state.db.arbitrage_signal_repository();
    
    // Get total count and data in parallel
    let (count_result, data_result) = tokio::join!(
        repo.count_by_chain(&chain),
        repo.get_by_chain(&chain, limit, offset)
    );
    
    match (count_result, data_result) {
        (Ok(total_count), Ok(signals)) => {
            Json(PaginatedResponse::new(signals, page, page_size, Some(total_count)))
        }
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!("Failed to fetch arbitrage signals by chain: {}", e);
            Json(PaginatedResponse::new(vec![], page, page_size, Some(0)))
        }
    }
}

pub async fn get_signals_by_pair(
    State(state): State<AppState>,
    Path(pair): Path<String>,
    Query(params): Query<ChainQuery>,
) -> Json<PaginatedResponse<ArbitrageSignal>> {
    let (page, page_size) = params.pagination.sanitize();
    let (offset, limit) = params.pagination.to_offset_limit();
    
    info!(
        pair = %pair,
        page = %page, 
        page_size = %page_size,
        "Fetching arbitrage signals by pair"
    );
    
    let repo = state.db.arbitrage_signal_repository();
    
    // Get total count and data in parallel
    let (count_result, data_result) = tokio::join!(
        repo.count_by_pair(&pair),
        repo.get_by_pair(&pair, limit, offset)
    );
    
    match (count_result, data_result) {
        (Ok(total_count), Ok(signals)) => {
            Json(PaginatedResponse::new(signals, page, page_size, Some(total_count)))
        }
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!("Failed to fetch arbitrage signals by pair: {}", e);
            Json(PaginatedResponse::new(vec![], page, page_size, Some(0)))
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_signals))
        .route("/by_chain/:chain", get(get_signals_by_chain))
        .route("/by_pair/:pair", get(get_signals_by_pair))
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

    #[test]
    fn test_chain_query_deserialization() {
        let query = "page=3&page_size=25";
        let parsed: ChainQuery = serde_urlencoded::from_str(query).unwrap();
        
        assert_eq!(parsed.pagination.page, Some(3));
        assert_eq!(parsed.pagination.page_size, Some(25));
    }

    #[test]
    fn test_chain_query_defaults() {
        let query = "";
        let parsed: ChainQuery = serde_urlencoded::from_str(query).unwrap();
        
        assert_eq!(parsed.pagination.page, None);
        assert_eq!(parsed.pagination.page_size, None);
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
    fn test_pair_query_parameter_handling() {
        // Test with pagination parameters for pair endpoint
        let query = "page=2&page_size=10";
        let parsed: ChainQuery = serde_urlencoded::from_str(query).unwrap();
        assert_eq!(parsed.pagination.page, Some(2));
        assert_eq!(parsed.pagination.page_size, Some(10));
        
        // Test empty query for pair endpoint
        let empty_query = "";
        let parsed_empty: ChainQuery = serde_urlencoded::from_str(empty_query).unwrap();
        assert_eq!(parsed_empty.pagination.page, None);
        assert_eq!(parsed_empty.pagination.page_size, None);
    }
}
