use color_eyre::eyre::{self, ContextCompat as _, OptionExt as _};
use num_bigint::{BigInt, BigUint};
use num_rational::BigRational;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

use crate::{
    spot_prices::SpotPrices,
    state::{self, pair::Pair},
    strategy,
};
use num_traits::{CheckedAdd as _, CheckedMul as _, CheckedSub as _, FromPrimitive as _};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealizedProfit {
    /// The token pair for which the profit was realized
    pub pair: Pair,
    /// The slow chain swap that generated the profit
    pub slow_swap: state::Swap,
    /// The fast chain swap that generated the profit
    pub fast_swap: state::Swap,
    /// Token A -> USDC and Token B -> USDC prices used to calculate realized profit
    pub token_usdc_prices: (f64, f64),
    /// ETH to USDC price used for converting gas costs to USDC
    pub gas_price_usdc: f64,
    /// Total gas consumed by both transactions in wei (slow + fast chain)
    pub gas_amount_eth: BigUint,
    /// Total gas cost converted to USDC using the ETH-USDC price
    pub gas_amount_usdc: BigUint,
    /// Surplus amounts in token A and token B respectively
    pub surplus: (BigUint, BigUint),
    /// USDC value of the token amounts
    pub usdc_amounts: (BigUint, BigUint),
    /// Total profit in USDC after converting surplus tokens using pessimistic prices
    pub total_usdc: BigUint,
}

impl RealizedProfit {
    /// Calculate realized profit from slow and fast chain swaps and spot prices.
    ///
    /// Calculates the realized profit by:
    /// 1. Determining the surplus amounts of each token (output - input)
    /// 2. Converting these surplus amounts to USDC using pessimistic prices
    /// 3. Summing the USDC values to get the total realized profit
    ///
    /// Returns a RealizedProfit struct containing
    /// - Total USDC profit
    /// - Surplus token amounts
    /// - The token pair
    /// - The slow and fast chain swaps
    /// - The USDC prices used for conversion
    pub fn try_from_swaps(
        slow_swap: &state::Swap,
        fast_swap: &state::Swap,
        prices_a_usdc: Option<SpotPrices>,
        prices_b_usdc: Option<SpotPrices>,
        prices_eth_usdc: Option<SpotPrices>,
    ) -> eyre::Result<Self> {
        let slow_pair = slow_swap.get_pair();
        let (amounts_a, amounts_b) =
            Self::try_amounts_by_tokens_a_b(slow_swap, fast_swap, &slow_pair)?;
        let surplus = try_surplus(amounts_a, amounts_b, &slow_pair)?;

        let (usdc_amounts, token_usdc_prices) = {
            let (a_usdc, price_a_usdc) = try_mul_amount_usdc_price(&surplus.0, &prices_a_usdc)?;
            let (b_usdc, price_b_usdc) = try_mul_amount_usdc_price(&surplus.1, &prices_b_usdc)?;
            ((a_usdc, b_usdc), (price_a_usdc, price_b_usdc))
        };

        let total_usdc = usdc_amounts
            .0
            .checked_add(&usdc_amounts.1)
            .wrap_err_with(|| {
                format!(
                    "total_profit_usdc failed: min_usdc_amounts {} + {}",
                    usdc_amounts.0, usdc_amounts.1
                )
            })?;

        let gas_amount_eth = slow_swap.gas_cost_eth.clone() + fast_swap.gas_cost_eth.clone();
        // TODO: remove this option after providing gas prices
        let (gas_amount_usdc, gas_price_usdc) = match prices_eth_usdc {
            Some(prices_eth_usdc) => {
                try_mul_amount_usdc_price(&gas_amount_eth, &Some(prices_eth_usdc))?
            }
            None => (gas_amount_eth.clone(), 1.0f64),
        };

        Ok(RealizedProfit {
            total_usdc,
            token_usdc_prices,
            pair: slow_pair.clone(),
            surplus,
            usdc_amounts,
            slow_swap: slow_swap.clone(),
            fast_swap: fast_swap.clone(),
            gas_amount_eth,
            gas_price_usdc,
            gas_amount_usdc,
        })
    }

    /// Return two tuples of (amount_in, amount_out) for token A, B respectively
    #[allow(clippy::type_complexity)]
    fn try_amounts_by_tokens_a_b<'a>(
        slow_sim: &'a state::Swap,
        fast_sim: &'a state::Swap,
        pair: &Pair,
    ) -> eyre::Result<((&'a BigUint, &'a BigUint), (&'a BigUint, &'a BigUint))> {
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
                "pair tokens {} don't match swap tokens: {:?}",
                pair,
                slow_sim
            ))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExpectedProfit {
    pub pair: Pair,
    /// Raw surplus in token amounts (a, b respectively) from swaps, before slippage and congestion
    pub surplus: (BigUint, BigUint),
    /// Max amount of tokens paid in slippage
    pub max_slippage_token_amounts: (BigUint, BigUint),
    /// Minimum amount of tokens after deducting max slippage from surplus and applying congestion discount
    pub min_token_amounts: (BigUint, BigUint),
    /// Token A -> USDC and Token B -> USDC prices used to calculate minimum USDC amount
    pub token_usdc_prices: (f64, f64),
    /// USDC value of the minimum token amounts
    pub min_usdc_amounts: (BigUint, BigUint),
    /// Total USDC value of the minimum token amounts
    pub min_total_amount_usdc: BigUint,
    // TODO: slow, fast basefee
    pub base_fee: u64,
    pub gas_cost_amounts: (BigUint, BigUint),
    pub gas_cost_eth: (BigUint, BigUint),
    pub total_gas_cost_eth: BigUint,
    pub eth_usdc_price: f64,
    pub gas_cost_usdc: (BigUint, BigUint),
    pub total_gas_cost_usdc: BigUint,
}

impl ExpectedProfit {
    /// Calculate expected profit from slow and fast chain swap simulations.
    ///
    /// The profit calculation pipeline:
    /// 1. Extracts token amounts from both swap simulations based on the token pair
    /// 2. Calculates raw surplus (difference between outputs and inputs for each token)
    /// 3. Applies maximum slippage discount based on fast simulation outputs
    /// 4. Applies congestion risk discount to the surplus after slippage
    /// 5. Determines pessimistic USDC prices (minimum for token_a, maximum for token_b)
    /// 6. Converts final token amounts to USDC values using the pessimistic prices
    ///
    /// Returns a structured ExpectedProfit containing:
    /// - Raw surplus in token amounts before any discounts
    /// - Maximum slippage that could be lost per token
    /// - Minimum guaranteed token amounts after slippage and congestion discount
    /// - USDC prices used for conversion
    /// - Final minimum USDC amounts achievable for the arbitrage
    pub fn try_from_swaps(
        slow_sim: &strategy::Swap,
        fast_sim: &strategy::Swap,
        prices_a_usdc: &Option<SpotPrices>,
        prices_b_usdc: &Option<SpotPrices>,
        prices_eth_usdc: &Option<SpotPrices>,
        max_slippage_bps: u64,
        congestion_risk_discount_bps: u64,
        base_fee: u64,
    ) -> eyre::Result<Self> {
        // Extract (amount_in, amount_out) pairs for token A and token B from the swap simulations
        let (amounts_a, amounts_b) = Self::try_amounts_by_tokens_a_b(slow_sim, fast_sim)?;
        let pair = slow_sim.get_pair();

        // TODO: use slow basefee
        let slow_gas_cost_eth = slow_sim
            .gas_cost
            .checked_mul(&BigUint::from(base_fee))
            .ok_or_eyre("failed calculating gas_cost")?;

        // TODO: use fast basefee
        let fast_gas_cost_eth = fast_sim
            .gas_cost
            .checked_mul(&BigUint::from(base_fee))
            .ok_or_eyre("failed calculating gas_cost")?;

        let total_gas_cost_eth = slow_gas_cost_eth
            .checked_add(&fast_gas_cost_eth)
            .wrap_err_with(|| {
                format!(
                    "total_gas_cost_eth failed: slow_gas_cost_eth {} + fast_gas_cost_eth {}",
                    slow_gas_cost_eth, fast_gas_cost_eth
                )
            })?;

        let (gas_cost_usdc_amounts, eth_usdc_price) = {
            let (slow_gas_cost, price_eth_usdc) =
                try_mul_amount_usdc_price(&slow_gas_cost_eth, &prices_eth_usdc)?;
            let (fast_gas_cost, _) =
                try_mul_amount_usdc_price(&fast_gas_cost_eth, &prices_eth_usdc)?;
            ((slow_gas_cost, fast_gas_cost), (price_eth_usdc))
        };

        let total_gas_cost_usdc = gas_cost_usdc_amounts
            .0
            .checked_add(&gas_cost_usdc_amounts.1)
            .wrap_err_with(|| {
                format!(
                    "total_gas_cost_usdc failed: gas_cost_usdc_amounts {} + {}",
                    gas_cost_usdc_amounts.0, gas_cost_usdc_amounts.1
                )
            })?;

        // Step 1: Calculate raw surplus (output - input for each token)
        let surplus = try_surplus(amounts_a, amounts_b, &pair)?;

        // Step 2: Calculate maximum slippage amounts as a percentage of the fast chain outputs
        let max_slippage_token_amounts =
            Self::try_max_slippage_amounts(amounts_a.1, amounts_b.1, max_slippage_bps)?;

        // Step 3: Apply congestion risk discount to the surplus after accounting for slippage
        // This represents the minimum guaranteed amount if congestion occurs on-chain
        let min_token_amounts = Self::apply_congestion_discount(
            &surplus,
            &max_slippage_token_amounts,
            congestion_risk_discount_bps,
        )?;

        // Step 4: Determine pessimistic USDC prices for each token andonvert minimum token amounts to USDC
        let (min_usdc_amounts, token_usdc_prices) = {
            let (a_usdc, price_a_usdc) =
                try_mul_amount_usdc_price(&min_token_amounts.0, prices_a_usdc)?;
            let (b_usdc, price_b_usdc) =
                try_mul_amount_usdc_price(&min_token_amounts.1, prices_b_usdc)?;
            ((a_usdc, b_usdc), (price_a_usdc, price_b_usdc))
        };

        // Step 5: Calculate total amount of USDC after accounting for slippage and congestion risk
        let min_total_amount_usdc = min_usdc_amounts
            .0
            .checked_add(&min_usdc_amounts.1)
            .wrap_err_with(|| {
                format!(
                    "total_profit_usdc failed: min_usdc_amounts {} + {}",
                    min_usdc_amounts.0, min_usdc_amounts.1
                )
            })?;

        Ok(ExpectedProfit {
            surplus,
            max_slippage_token_amounts,
            min_token_amounts,
            token_usdc_prices,
            eth_usdc_price: eth_usdc_price,
            min_usdc_amounts,
            min_total_amount_usdc,
            pair,
            base_fee,
            gas_cost_amounts: (slow_sim.gas_cost.clone(), fast_sim.gas_cost.clone()),
            gas_cost_eth: (slow_gas_cost_eth, fast_gas_cost_eth),
            total_gas_cost_eth,
            gas_cost_usdc: gas_cost_usdc_amounts,
            total_gas_cost_usdc,
        })
    }

    /// Return two tuples of (amount_in, amount_out) for token A, B respectively
    #[allow(clippy::type_complexity)]
    fn try_amounts_by_tokens_a_b<'a>(
        slow_sim: &'a strategy::Swap,
        fast_sim: &'a strategy::Swap,
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

    /// Calculate the maximum token amounts that could be lost to slippage.
    ///
    /// Computes the difference between the expected output amounts and the minimum
    /// amounts after applying the maximum slippage tolerance. This represents the
    /// worst-case slippage for each token in the arbitrage.
    fn try_max_slippage_amounts(
        out_a: &BigUint,
        out_b: &BigUint,
        max_slippage_bps: u64,
    ) -> eyre::Result<(BigUint, BigUint)> {
        let min_out_a = bps_discount(out_a, max_slippage_bps);
        let min_out_b = bps_discount(out_b, max_slippage_bps);

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

    /// Apply congestion risk discount to the surplus after accounting for slippage.
    ///
    /// Calculates the minimum guaranteed profit by:
    /// 1. Subtracting maximum slippage amounts from the raw surplus
    /// 2. Applying the congestion risk discount factor to the result
    ///
    /// This accounts for the risk that transactions may execute at worse prices
    /// due to competition for the same liquidity pools on-chain.
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
            self.token_usdc_prices.0,
            self.token_usdc_prices.1,
        )
    }
}

/// Apply a basis points discount to an amount.
///
/// Calculates `amount * (10000 - slippage_bps) / 10000`, effectively reducing
/// the amount by the given percentage. For example, 50 bps (0.5%) discount on
/// 1000 returns 995.
pub fn bps_discount(amount: &BigUint, slippage_bps: u64) -> BigUint {
    let slippage_multiplier = BigUint::from(10000u64 - slippage_bps);
    (amount * slippage_multiplier) / BigUint::from(10000u64)
}

/// Calculate the raw surplus from input/output amounts for both tokens.
///
/// Computes `(out_a - in_a, out_b - in_b)` representing the net gain in each
/// token from the cross-chain arbitrage. Both values should be positive for
/// a profitable trade.
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

/// Convert a token amount to USDC using spot prices.
///
/// If prices are provided, multiplies the amount by the minimum (pessimistic) price
/// and adjusts for decimal differences between the tokens. If prices are None
/// (indicating the token is already USDC), returns the amount unchanged.
///
/// Returns a tuple of (usdc_amount, price_used).
pub fn try_mul_amount_usdc_price(
    amount: &BigUint,
    prices: &Option<SpotPrices>,
) -> eyre::Result<(BigUint, f64)> {
    // If direct USDC pricing is available use it; otherwise this is USDC
    match prices {
        Some(prices) => {
            let price = prices.min_price;
            let amount_usdc = try_mul_biguint_f64(amount, price, &prices.pair)?;

            Ok((amount_usdc, price))
        }
        None => Ok((amount.clone(), 1.0f64)),
    }
}

/// Multiply a token amount by a price, adjusting for decimal differences.
///
/// Handles the conversion between tokens with different decimal precisions.
/// For example, when converting from an 18-decimal token to 6-decimal USDC,
/// divides by 10^12 to normalize the result.
fn try_mul_biguint_f64(amount: &BigUint, price: f64, pair: &Pair) -> eyre::Result<BigUint> {
    let price =
        BigRational::from_f64(price).ok_or_eyre("failed to convert price to BigRational")?;

    // Get tokens ordered with non-USDC token first
    let (token_a, token_b) = pair.token_a_b_adjusted_for_usdc();
    let decimals_diff = (token_a.decimals as i32) - (token_b.decimals as i32);

    // Create decimal adjustment to normalize amount for the decimal difference
    // Example: if token_a has 18 decimals and token_b (USDC) has 6 decimals:
    //   decimals_diff = 12
    //   adjustment = 1 / 10^12 (divide the amount by 10^12)
    let decimal_adjustment = if decimals_diff >= 0 {
        // token_a has more decimals: divide by 10^decimals_diff
        BigRational::new(BigInt::from(1), BigInt::from(10).pow(decimals_diff as u32))
    } else {
        // token_a has fewer decimals: multiply by 10^|decimals_diff|
        BigRational::new(
            BigInt::from(10).pow(decimals_diff.unsigned_abs()),
            BigInt::from(1),
        )
    };

    // Normalize the amount and multiply by price: (amount * adjustment) * price
    let amount_rational = BigRational::from_integer(BigInt::from(amount.clone()));

    let amount_usdc = amount_rational
        .checked_mul(&price)
        .wrap_err("failed to multiply amount by price")?
        .checked_mul(&decimal_adjustment)
        .wrap_err("failed to adjust amount * price by decimals")?
        .to_integer()
        .to_biguint()
        .ok_or_eyre("failed to convert USDC amount to BigUint")?;

    Ok(amount_usdc)
}
