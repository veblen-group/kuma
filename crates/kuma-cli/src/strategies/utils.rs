use color_eyre::eyre::{self, Ok};
use tycho_simulation::{models::Token, protocol::state::ProtocolSim};

use crate::strategies::TradeAmounts;

pub(crate) fn get_pool_limits(
    asset_state: Box<dyn ProtocolSim>,
    token_a: &Token,
    token_b: &Token,
) -> eyre::Result<TradeAmounts> {
    // gets the maximum amount in token a to token b and vice versa
    let limit_a_to_b = asset_state
        .get_limits(token_a.address.clone(), token_b.address.clone())?
        .0;
    let limit_b_to_a = asset_state
        .get_limits(token_b.address.clone(), token_a.address.clone())?
        .0;
    Ok(TradeAmounts {
        sell_a: limit_a_to_b.clone(),
        sell_b: limit_b_to_a.clone(),
    })
}

// calculates the minimum amount in token a to token b and vice versa
pub(crate) fn get_amounts_limits(
    asset_state: Box<dyn ProtocolSim>,
    token_a: &Token,
    token_b: &Token,
    inventory: &TradeAmounts,
) -> eyre::Result<TradeAmounts> {
    let protocol_limits = get_pool_limits(asset_state, token_a, token_b)?;
    let inventory_limits = inventory.clone();
    Ok(TradeAmounts {
        sell_a: protocol_limits.sell_a.min(inventory_limits.sell_a),
        sell_b: protocol_limits.sell_b.min(inventory_limits.sell_b),
    })
}
