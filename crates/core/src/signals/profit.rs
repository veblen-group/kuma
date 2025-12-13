use color_eyre::eyre::{self, ContextCompat as _};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use tycho_common::models::token::Token;

use crate::{signals::bps_discount, spot_prices::SpotPrices, strategy::Swap, try_mul_usdc_price};
use num_traits::CheckedSub as _;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExpectedProfit {
    /// Expected profit in USDC
    pub usdc_amount: BigUint,
    /// Expected profit in token amounts
    pub token_amounts: (BigUint, BigUint),
    pub token_a: Token,
    pub token_b: Token,
}

impl ExpectedProfit {
    pub fn try_from_swaps(
        slow_sim: &Swap,
        fast_sim: &Swap,
        prices_a_usdc: &Option<SpotPrices>,
        prices_b_usdc: &Option<SpotPrices>,
        max_slippage_bps: u64,
        congestion_risk_discount_bps: u64,
    ) -> eyre::Result<Self> {
        // Calculate worst-case slippage
        let min_slow_amount_out = bps_discount(&slow_sim.amount_out, max_slippage_bps);
        let min_fast_amount_out = bps_discount(&fast_sim.amount_out, max_slippage_bps);

        let after_max_slippage_a = min_fast_amount_out
            .checked_sub(&slow_sim.amount_in)
            .wrap_err("min surplus of token a cannot be negative")?;
        let after_max_slippage_b = min_slow_amount_out
            .checked_sub(&fast_sim.amount_in)
            .wrap_err("min surplus of token b cannot be negative")?;

        // Calculate congestion risk discount
        let after_discount_a_surplus =
            bps_discount(&after_max_slippage_a, congestion_risk_discount_bps);
        let after_discount_b_surplus =
            bps_discount(&after_max_slippage_b, congestion_risk_discount_bps);

        // Convert to USDC

        let after_discount_a_usdc = if let Some(prices_a_usdc) = prices_a_usdc {
            try_mul_usdc_price(after_discount_a_surplus.clone(), prices_a_usdc)?
        } else {
            BigUint::ZERO
        };
        let after_discount_b_usdc = if let Some(prices_b_usdc) = prices_b_usdc {
            try_mul_usdc_price(after_discount_b_surplus.clone(), prices_b_usdc)?
        } else {
            BigUint::ZERO
        };

        Ok(ExpectedProfit {
            usdc_amount: after_discount_a_usdc + after_discount_b_usdc,
            token_amounts: (after_discount_a_surplus, after_discount_b_surplus),
            token_a: slow_sim.token_in.clone(),
            token_b: slow_sim.token_out.clone(),
        })
    }
}

impl Display for ExpectedProfit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tokens Expected Profit: {} ({}) {} ({})
            Expected Profit USD: {}
            ",
            self.token_amounts.0, "Token A", self.token_amounts.1, "Token B", self.usdc_amount,
        )
    }
}
