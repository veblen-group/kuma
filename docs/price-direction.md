# Price Direction & Spot Prices

This document explains how spot prices are computed and what direction (base/quote) they represent. Getting this wrong leads to prices like 0.000333 instead of 3000 for ETH/USDC — a common source of confusion.

## The call chain

```
try_make_sorted_spot_prices(state, pair)
  └─ pair.token_a_b_adjusted_for_usdc()  →  (base, quote)
  └─ pool.spot_price(base, quote)         →  f64  (quote per base)
```

## `token_a_b_adjusted_for_usdc` — USDC is always the quote

`crates/core/src/state/pair.rs` — `Pair::token_a_b_adjusted_for_usdc()`

| Strategy pair | Returns (base, quote) | Price meaning |
|---------------|----------------------|---------------|
| USDC / WETH (token_a = USDC) | (WETH, USDC) | USDC per WETH ≈ 3000 |
| WETH / USDC (token_b = USDC) | (WETH, USDC) | USDC per WETH ≈ 3000 |
| WETH / WBTC (no USDC) | (WETH, WBTC) | WBTC per WETH ≈ 0.03 |

USDC is always placed as the quote so all prices are expressed in a human-readable "per USDC" or "in USDC terms" form.

## `ProtocolSim::spot_price(base, quote)` — always returns quote per base

`tycho-simulation` — `UniswapV3State::spot_price(a, b)`

Internally, Uniswap V3's `sqrtPriceX96` encodes `sqrt(token1/token0)` where token0 < token1 by address. The implementation checks the address order and either applies or inverts the sqrtPrice formula so that **the return value is always `b per a` regardless of which token has the lower address**.

```rust
fn spot_price(&self, a: &Token, b: &Token) -> Result<f64> {
    let price = if a < b {                         // a is token0
        sqrt_price_q96_to_f64(sqrt_price, a.decimals, b.decimals)?  // b/a
    } else {
        1.0 / sqrt_price_q96_to_f64(sqrt_price, b.decimals, a.decimals)?  // 1/(a/b) = b/a
    };
    Ok(add_fee_markup(price, self.fee()))
}
```

## SpotPrices struct

`crates/core/src/spot_prices.rs`

```rust
pub struct SpotPrices {
    pub pair: Pair,          // stores the *strategy* pair (token_a, token_b)
    pub min_price: f64,      // quote per base — best price for the buyer
    pub max_price: f64,      // quote per base — best price for the seller
    pub min_pool_id: PoolId,
    pub max_pool_id: PoolId,
    pub block_height: u64,
    pub chain: Chain,
}
```

Note: `pair` stores the original strategy ordering (token_a/token_b), **not** the adjusted base/quote ordering. The price values however are always quote-per-base.

## In the webapp

`webapp/src/components/spot_prices/` — both card headers and the chart Y-axis show the price direction.

The frontend mirrors `token_a_b_adjusted_for_usdc()`:

```ts
// USDC is always the quote (price unit)
const priceFrom = strategy.tokenA === 'USDC' ? strategy.tokenB : strategy.tokenA
const priceTo   = strategy.tokenA === 'USDC' ? strategy.tokenA : strategy.tokenB
```

The chart Y-axis label shows `{priceTo} / {priceFrom}` (e.g. `USDC / WETH`) to make the unit explicit.
