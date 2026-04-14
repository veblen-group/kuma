# Expected Profit & Discounting

`crates/core/src/signals/profit.rs`

Profit calculation converts a raw token surplus into a USDC-denominated expected profit after applying three layers of discounts: slippage, congestion risk, and gas cost. For full API detail see `kuma_core::signals::profit` in the rustdoc.

## Raw surplus

For a trade that sells token A on the slow chain and buys it back on the fast chain:

```
surplus_A = amount_out_fast - amount_in_slow
```

The intermediate token B amounts cancel out because the fast leg's input is set to the slow leg's output. See `tap6-proposal.md` §Computing Trade Surplus in the source repository for the full derivation.

## Config flags — `ignore_*_in_profit`

Each discount can be disabled for testing without removing the underlying calculation. Values are still tracked and logged even when ignored:

| Flag | What it skips |
|------|--------------|
| `ignore_slippage_in_profit` | Slippage discount |
| `ignore_congestion_fee_in_profit` | Congestion risk discount |
| `ignore_gas_costs_in_profit` | Gas cost deduction |
| `ignore_usdc_conversion_in_profit` | USDC price lookup (uses raw token amounts) |

## `same_outcome` — signal dedup

`ExpectedProfit::same_outcome()` compares two signals to see if they'd produce the same trade. Used by the strategy worker to avoid re-queuing a signal when the fast chain has a new block but the price hasn't meaningfully changed. Two signals have the same outcome if they share the same direction and exact `min_total_amount_usdc` value.
