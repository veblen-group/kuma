# Expected Profit & Discounting

`crates/core/src/signals/profit.rs`

Profit calculation converts a raw token surplus into a USDC-denominated expected profit after applying three layers of discounts: slippage, congestion risk, and gas cost.

## Raw surplus

For a trade that sells token A on the slow chain and buys it back on the fast chain:

```
surplus_A = amount_out_fast - amount_in_slow
```

The intermediate token B amounts cancel out because the fast leg's input is set to the slow leg's output. See [tap6-proposal.md](tap6-proposal.md) §Computing Trade Surplus for the full derivation.

## Discount chain — `ExpectedProfit`

`ExpectedProfit::try_from_swaps()` applies discounts in this order:

### 1. Slippage

`max_slippage_bps` (basis points, e.g. 50 = 0.5%) is applied to both legs' `amount_out` values, modelling worst-case execution:

```
effective_amount_out = amount_out * (1 - slippage_bps / 10_000)
```

The fast leg's `amount_in` is also reduced to match the slippage-adjusted slow output, so intermediate amounts still cancel.

### 2. Congestion risk

`congestion_risk_discount_bps` (e.g. 200 = 2%) is a flat discount to the surplus, modelling the probability that another transaction takes the pool first:

```
E[profit | congestion] = surplus * (1 - congestion_bps / 10_000)
```

### 3. Gas cost

Gas cost = `(slow_gas_units + fast_gas_units) * base_fee_wei`, converted to USDC via the ETH/USDC spot price:

```
E[profit | slippage, gas] = discounted_surplus_usdc - gas_cost_usdc
```

### Config flags — `ignore_*_in_profit`

Each discount can be disabled for testing without removing the underlying calculation. Values are still tracked and logged even when ignored:

| Flag | What it skips |
|------|--------------|
| `ignore_slippage_in_profit` | Slippage discount |
| `ignore_congestion_fee_in_profit` | Congestion risk discount |
| `ignore_gas_costs_in_profit` | Gas cost deduction |
| `ignore_usdc_conversion_in_profit` | USDC price lookup (uses raw token amounts) |

## USDC conversion

Surplus amounts are converted to USDC using **pessimistic (min) spot prices** from the slow chain precompute:

- Token A surplus → multiplied by `prices_a_usdc.min_price`
- Token B surplus → multiplied by `prices_b_usdc.min_price`
- If either token is USDC, the price is `1.0` (no conversion needed)

## `same_outcome` — signal dedup

`ExpectedProfit::same_outcome()` compares two signals to see if they'd produce the same trade. Used by the strategy worker to avoid re-queuing a signal when the fast chain has a new block but the price hasn't meaningfully changed. Two signals have the same outcome if their direction and profitability bucket are identical.

## `RealizedProfit`

After a trade executes, `RealizedProfit` is calculated from actual on-chain receipts:

- Amounts are parsed from `Transfer` events in the transaction logs (`state::Swap::try_from_receipts`).
- Gas cost is `gas_used * effective_gas_price` from the receipt.
- Surplus and USDC conversion use the same logic as `ExpectedProfit` but with real amounts.

`RealizedProfit` is persisted to the database alongside the trade result.
