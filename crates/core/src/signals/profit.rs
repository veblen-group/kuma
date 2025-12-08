use color_eyre::eyre::{self};
use num_bigint::BigUint;

use crate::strategy::Swap;

pub struct ExpectedProfit {
    /// Expected profit in USDC
    pub usdc_amount: BigUint,
    /// Expected profit in token amounts
    pub token_amounts: (BigUint, BigUint),
    /// Maximum token amount lost to slippage
    pub max_slippage: (BigUint, BigUint),
    /// Risk discount in token amounts
    pub congestion_risk_discount: (BigUint, BigUint),
}

impl ExpectedProfit {
    pub fn try_from_swaps(
        _slow_sim: &Swap,
        _fast_sim: &Swap,
        _max_slippage_bps: u64,
        _congestion_risk_discount_bps: u64,
    ) -> eyre::Result<Self> {
        // TODO: calculate max slippage costs
        // TODO: calculate congestion risk discount
        // TODO: compound two separate congestion risks, one for each side
        // TODO: calculate final usdc amount

        Ok(ExpectedProfit {
            usdc_amount: todo!(),
            token_amounts: todo!(),
            max_slippage: todo!(),
            congestion_risk_discount: todo!(),
        })
    }
}
