use color_eyre::eyre::{self, Context as _, ContextCompat as _, OptionExt as _};
use num_bigint::{BigInt, BigUint};
use num_rational::BigRational;
use num_traits::{CheckedMul as _, FromPrimitive as _};

use crate::spot_prices::SpotPrices;

pub mod chain;
pub mod collector;
pub mod config;
pub mod database;
pub mod encoder;
pub mod signals;
pub mod spot_prices;
pub mod state;
pub mod strategy;

fn try_mul_usdc_price(amount: BigUint, usdc_prices: &SpotPrices) -> eyre::Result<BigUint> {
    let price = BigRational::from_f64(
        usdc_prices
            .try_pessimistic_usdc_price()
            .wrap_err_with(|| format!("failed to get price for {}", usdc_prices.pair))?,
    )
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
