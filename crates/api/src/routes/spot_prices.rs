use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tracing::info;

use crate::{
    models::PairPrice,
    state::AppState,
};

#[derive(Deserialize)]
pub struct SpotPriceQuery {
    pub block_height: u64,
    pub pair: Option<String>, // Format: "TokenA-TokenB"
}

pub async fn get_spot_prices(
    State(state): State<AppState>,
    Query(params): Query<SpotPriceQuery>,
) -> Json<Vec<PairPrice>> {
    info!(block_height = %params.block_height, pair = ?params.pair, "Fetching spot prices");

    let repo = state.db.pair_price_repository();
    
    match repo.get_by_block_height(params.block_height, params.pair.as_deref()).await {
        Ok(prices) => Json(prices),
        Err(e) => {
            tracing::error!("Failed to fetch spot prices: {}", e);
            Json(vec![])
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
        let query = "block_height=19500000&pair=WETH-USDC";
        let parsed: SpotPriceQuery = serde_urlencoded::from_str(query).unwrap();
        
        assert_eq!(parsed.block_height, 19500000);
        assert_eq!(parsed.pair, Some("WETH-USDC".to_string()));
    }

    #[test]
    fn test_spot_price_query_without_pair() {
        let query = "block_height=19500000";
        let parsed: SpotPriceQuery = serde_urlencoded::from_str(query).unwrap();
        
        assert_eq!(parsed.block_height, 19500000);
        assert_eq!(parsed.pair, None);
    }
}
