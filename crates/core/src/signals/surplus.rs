use std::fmt::Display;

use color_eyre::eyre::{self, ContextCompat as _};
use num_bigint::BigUint;
use num_traits::CheckedSub as _;
use serde::{Deserialize, Serialize};

use crate::{
    signals::{bps_discount, try_mul_usdc_price},
    spot_prices::SpotPrices,
    state::pair::Pair,
    strategy::Swap,
};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Surplus {
    pub slow_pair: Pair,
    pub token_amounts: (BigUint, BigUint),
    pub token_amounts_after_slippage: (BigUint, BigUint),
    pub usdc_amounts: (BigUint, BigUint),
}

impl Surplus {
    pub fn try_from_swaps(
        slow_sim: &Swap,
        fast_sim: &Swap,
        prices_a_usdc: &SpotPrices,
        prices_b_usdc: &SpotPrices,
        max_slippage_bps: u64,
    ) -> eyre::Result<Self> {
        // Calculate raw surplus
        let surplus_a = fast_sim
                .amount_out
                .checked_sub(&slow_sim.amount_in)
                .wrap_err_with(|| {
                    format!(
                        "surplus of token a cannot be negative: fast.amount_out - slow.amount_in = {} - {} ",
                        fast_sim.amount_out, slow_sim.amount_in
                    )
                })?;
        let surplus_b = slow_sim
                .amount_out
                .checked_sub(&fast_sim.amount_in)
                .wrap_err_with(|| {
                    format!(
                        "surplus of token b cannot be negative: slow.amount_out={} - fast.amount_in={} ",
                        slow_sim.amount_out, fast_sim.amount_in
                    )
                })?;

        // Calculate worst-case slippage
        let min_slow_amount_out = bps_discount(&slow_sim.amount_out, max_slippage_bps);
        let min_fast_amount_out = bps_discount(&fast_sim.amount_out, max_slippage_bps);

        let after_max_slippage_a = min_fast_amount_out
            .checked_sub(&slow_sim.amount_in)
            .wrap_err("min surplus of token a cannot be negative")?;
        let after_max_slippage_b = min_slow_amount_out
            .checked_sub(&fast_sim.amount_in)
            .wrap_err("min surplus of token b cannot be negative")?;

        // Calculate USDC value
        let surplus_a_usdc = try_mul_usdc_price(surplus_a.clone(), prices_a_usdc)?;
        let surplus_b_usdc = try_mul_usdc_price(surplus_b.clone(), prices_b_usdc)?;

        Ok(Surplus {
            slow_pair: Pair::new(slow_sim.token_in.clone(), slow_sim.token_out.clone()),
            token_amounts: (surplus_a, surplus_b),
            token_amounts_after_slippage: (after_max_slippage_a, after_max_slippage_b),
            usdc_amounts: (surplus_a_usdc, surplus_b_usdc),
        })
    }
}

impl Display for Surplus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Surplus for pair {}: token_amounts=({}, {}), token_amounts_after_slippage=({}, {}), usdc_amounts=({}, {})",
            self.slow_pair,
            self.token_amounts.0,
            self.token_amounts.1,
            self.token_amounts_after_slippage.0,
            self.token_amounts_after_slippage.1,
            self.usdc_amounts.0,
            self.usdc_amounts.1
        )
    }
}
