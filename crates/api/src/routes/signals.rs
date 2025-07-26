use crate::{models::ArbitrageSignal, state::AppState};
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
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub async fn get_signals(
    State(state): State<AppState>,
    Query(params): Query<SignalQuery>,
) -> Json<Vec<ArbitrageSignal>> {
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);
    
    info!(block_height = %params.block_height, limit = %limit, offset = %offset, "Fetching arbitrage signals");
    
    let repo = state.db.arbitrage_signal_repository();
    
    match repo.get_by_block_height(params.block_height, limit, offset).await {
        Ok(signals) => Json(signals),
        Err(e) => {
            tracing::error!("Failed to fetch arbitrage signals: {}", e);
            Json(vec![])
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
        let query = "block_height=19500000&limit=10&offset=5";
        let parsed: SignalQuery = serde_urlencoded::from_str(query).unwrap();
        
        assert_eq!(parsed.block_height, 19500000);
        assert_eq!(parsed.limit, Some(10));
        assert_eq!(parsed.offset, Some(5));
    }

    #[test]
    fn test_signal_query_defaults() {
        let query = "block_height=19500000";
        let parsed: SignalQuery = serde_urlencoded::from_str(query).unwrap();
        
        assert_eq!(parsed.block_height, 19500000);
        assert_eq!(parsed.limit, None);
        assert_eq!(parsed.offset, None);
    }
}
