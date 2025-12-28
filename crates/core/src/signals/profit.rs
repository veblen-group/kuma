use color_eyre::eyre::{self, ContextCompat as _, OptionExt as _};
use num_bigint::{BigInt, BigUint};
use num_rational::BigRational;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

use crate::{spot_prices::SpotPrices, state::pair::Pair, strategy::Swap};
use num_traits::{CheckedAdd as _, CheckedMul as _, CheckedSub as _, FromPrimitive as _};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExpectedProfit {
    /// Raw surplus in token amounts (a, b respectively) from swaps, before slippage and congestion
    pub surplus: (BigUint, BigUint),
    /// Max amount of tokens paid in slippage
    pub max_slippage_token_amounts: (BigUint, BigUint),
    /// Minimum amount of tokens after deducting max slippage from surplus and applying congestion discount
    pub min_token_amounts: (BigUint, BigUint),
    /// Token A -> USDC and Token B -> USDC prices used to calculate minimum USDC amount
    pub usdc_prices: (f64, f64),
    /// USDC value of the minimum token amounts
    pub min_usdc_amounts: (BigUint, BigUint),
    pub pair: Pair,
}

impl ExpectedProfit {
    pub fn try_from_swaps(
        slow_sim: &Swap,
        fast_sim: &Swap,
        prices_a_b: &SpotPrices,
        prices_a_usdc: &Option<SpotPrices>,
        prices_b_usdc: &Option<SpotPrices>,
        max_slippage_bps: u64,
        congestion_risk_discount_bps: u64,
    ) -> eyre::Result<Self> {
        let (amounts_a, amounts_b) = Self::try_amounts_by_tokens_a_b(slow_sim, fast_sim)?;
        let pair = slow_sim.get_pair();

        // surplus
        let surplus = Self::try_surplus(amounts_a, amounts_b, &pair)?;

        // slippage
        let max_slippage_token_amounts =
            Self::try_max_slippage_amounts(&amounts_a.1, &amounts_b.1, max_slippage_bps)?;

        // congestion
        let min_token_amounts = Self::apply_congestion_discount(
            &surplus,
            &max_slippage_token_amounts,
            congestion_risk_discount_bps,
        )?;

        let usdc_prices = {
            let price_a_usdc = if let Some(prices_a_usdc) = prices_a_usdc {
                prices_a_usdc.try_pessimistic_usdc_price()?
            } else {
                prices_a_b.try_pessimistic_usdc_price()?
            };

            let price_b_usdc = if let Some(prices_b_usdc) = prices_b_usdc {
                prices_b_usdc.try_pessimistic_usdc_price()?
            } else {
                prices_a_b.try_pessimistic_usdc_price()?
            };

            (price_a_usdc, price_b_usdc)
        };

        // Convert to USDC
        let min_usdc_amounts = {
            let a_usdc = try_mul_biguint_f64(&min_token_amounts.0, usdc_prices.0)?;
            let b_usdc = try_mul_biguint_f64(&min_token_amounts.1, usdc_prices.1)?;
            (a_usdc, b_usdc)
        };

        Ok(ExpectedProfit {
            surplus,
            max_slippage_token_amounts,
            min_token_amounts,
            usdc_prices,
            min_usdc_amounts,
            pair,
        })
    }

    /// Return two tuples of (amount_in, amount_out) for token A, B respectively
    fn try_amounts_by_tokens_a_b<'a>(
        slow_sim: &'a Swap,
        fast_sim: &'a Swap,
    ) -> eyre::Result<((&'a BigUint, &'a BigUint), (&'a BigUint, &'a BigUint))> {
        let pair = slow_sim.get_pair();
        if pair.token_a().symbol == slow_sim.token_in.symbol {
            Ok((
                (&slow_sim.amount_in, &fast_sim.amount_out),
                (&fast_sim.amount_in, &slow_sim.amount_out),
            ))
        } else if pair.token_b().symbol == slow_sim.token_in.symbol {
            Ok((
                (&fast_sim.amount_in, &slow_sim.amount_out),
                (&slow_sim.amount_in, &fast_sim.amount_out),
            ))
        } else {
            Err(eyre::eyre!(
                "pair tokens {} don't match swap tokens: {}",
                pair,
                slow_sim
            ))
        }
    }

    fn try_surplus(
        (in_a, out_a): (&BigUint, &BigUint),
        (in_b, out_b): (&BigUint, &BigUint),
        pair: &Pair,
    ) -> eyre::Result<(BigUint, BigUint)> {
        // out_a - in_a
        let amount_a = out_a.checked_sub(in_a).wrap_err_with(|| {
            format!(
                "min_out_a {} cannot be less than in_a {} for token {}",
                out_a,
                in_a,
                pair.token_a().symbol,
            )
        })?;

        // out_b - in_b
        let amount_b = out_b.checked_sub(in_b).wrap_err_with(|| {
            format!(
                "min_out_b {} cannot be less than in_b {} for token {}",
                out_b,
                in_b,
                pair.token_b().symbol,
            )
        })?;
        Ok((amount_a, amount_b))
    }

    fn try_max_slippage_amounts(
        out_a: &BigUint,
        out_b: &BigUint,
        max_slippage_bps: u64,
    ) -> eyre::Result<(BigUint, BigUint)> {
        let min_out_a = bps_discount(&out_a, max_slippage_bps);
        let min_out_b = bps_discount(&out_b, max_slippage_bps);

        let amount_a = out_a.checked_sub(&min_out_a).wrap_err_with(|| {
            format!(
                "min_out_a {} cannot be less than out_a {}",
                min_out_a, out_a
            )
        })?;
        let amount_b = out_b.checked_sub(&min_out_b).wrap_err_with(|| {
            format!(
                "min_out_b {} cannot be less than out_b {}",
                min_out_b, out_b
            )
        })?;
        Ok((amount_a, amount_b))
    }

    fn apply_congestion_discount(
        surplus: &(BigUint, BigUint),
        max_slippage_token_amounts: &(BigUint, BigUint),
        congestion_risk_discount_bps: u64,
    ) -> eyre::Result<(BigUint, BigUint)> {
        let min_surplus_after_slippage_a = surplus
            .0
            .checked_sub(&max_slippage_token_amounts.0)
            .wrap_err_with(|| {
                format!(
                    "surplus {} cannot be less than max_slippage_token_amounts {}",
                    surplus.0, max_slippage_token_amounts.0
                )
            })?;

        let min_surplus_after_slippage_b = surplus
            .1
            .checked_sub(&max_slippage_token_amounts.1)
            .wrap_err_with(|| {
                format!(
                    "surplus {} cannot be less than max_slippage_token_amounts {}",
                    surplus.1, max_slippage_token_amounts.1
                )
            })?;

        // congestion_discount * (surplus - max_slippage) for each token
        Ok((
            bps_discount(&min_surplus_after_slippage_a, congestion_risk_discount_bps),
            bps_discount(&min_surplus_after_slippage_b, congestion_risk_discount_bps),
        ))
    }

    pub fn total_profit_usdc(&self) -> eyre::Result<BigUint> {
        self.min_usdc_amounts
            .0
            .checked_add(&self.min_usdc_amounts.1)
            .wrap_err_with(|| {
                format!(
                    "total_profit_usdc failed: min_usdc_amounts {} + {}",
                    self.min_usdc_amounts.0, self.min_usdc_amounts.1
                )
            })
    }
}

impl Display for ExpectedProfit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Expected Profit: {}\n\tMin USDC Amount: ({}, {})\n\tSurplus: ({}, {})\n\tMax Slippage Amounts: ({}, {})\n\tMin Token Amounts: ({}, {})\n\tUSDC Prices: ({}, {})",
            self.pair,
            self.min_usdc_amounts.0,
            self.min_usdc_amounts.1,
            self.surplus.0,
            self.surplus.1,
            self.max_slippage_token_amounts.0,
            self.max_slippage_token_amounts.1,
            self.min_token_amounts.0,
            self.min_token_amounts.1,
            self.usdc_prices.0,
            self.usdc_prices.1,
        )
    }
}

pub fn bps_discount(amount: &BigUint, slippage_bps: u64) -> BigUint {
    let slippage_multiplier = BigUint::from(10000u64 - slippage_bps);
    (amount * slippage_multiplier) / BigUint::from(10000u64)
}

fn try_mul_biguint_f64(amount: &BigUint, price: f64) -> eyre::Result<BigUint> {
    let price = BigRational::from_f64(price)
        .ok_or_eyre("failed to convert token A USDC price to BigRational")?;

    // TODO: are these conversions safe?
    let amount_usdc = price
        .checked_mul(&BigRational::from_integer(BigInt::from(amount.clone())))
        .wrap_err("surplus of token a cannot be converted to USDC")?
        .to_integer()
        .to_biguint()
        .ok_or_eyre("failed to multiply surplus a by usdc price")?;

    Ok(amount_usdc)
}
