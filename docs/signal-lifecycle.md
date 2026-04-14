# Signal Lifecycle

A signal starts as a detected price discrepancy and ends as a persisted trade result. This document traces the full path.

![Trade lifecycle](../../trade-lifecycle.png)

## 1. Slow chain block arrives → precompute

`crates/kumad/src/strategy/mod.rs` — `Worker::run()` slow stream arm

- `strategy.try_precompute(slow_state)` computes swap tables (`PoolSteps`) for every pool at `binary_search_steps` exponentially-spaced inventory amounts.
- Spot prices (`prices_a_b`, `prices_a_usdc`, `prices_b_usdc`, `prices_eth_usdc`) are extracted and stored in `Precomputes`.
- Slow chain spot prices are written to the database (fire-and-forget).
- A 75%-of-block-time submission deadline is set.

## 2. Fast chain block arrives → signal generation

`crates/kumad/src/strategy/mod.rs` — fast stream arm

- Fast chain spot prices are extracted and written to DB.
- `strategy.generate_signal(precompute, fast_state, ...)` runs a binary search over the precomputed slow chain swap table to find the size that maximises surplus on the fast chain.
- If no profitable trade exists → `Err`, spot prices are still persisted, no signal.
- If profitable → a `signals::CrossChainSingleHop` is built containing both swap simulations, expected profit, and all metadata needed for execution.

### Signal dedup

Before queuing a signal, the new signal's `expected_profit` is compared to `prev_signal` via `same_outcome()`. If the outcome is identical (same direction, same profitability bucket) the signal is **dropped** — no DB write, no emission. This prevents flooding execution with redundant signals when multiple fast blocks arrive without a meaningful price change.

## 3. Submission deadline fires → signal emitted

When the 75% timer fires and `curr_signal` is set, the signal is:
1. Sent to the execution worker via `signal_tx`.
2. Saved as `prev_signal` (for future dedup comparisons).

## 4. Execution worker receives signal → trade

`crates/kumad/src/execution/mod.rs` — `Worker::run()` signal stream arm

- Only one trade runs at a time. If a trade is already in-flight, the new signal is dropped.
- `signal.try_promote()` encodes the transactions: `create_solution()` → `encode_solution()` → `encode_tycho_router_call()`.
- The resulting `Trade` is run via `trade.run()`.

## 5. Trade execution → sequential submission

`crates/core/src/trade.rs` — `Trade::run()`

1. Estimate gas for both legs.
2. Submit slow chain transaction.
3. Wait for confirmation receipt.
4. If slow chain confirmed → submit fast chain transaction.
5. Parse `Transfer` logs from receipts into `state::Swap` to compute `RealizedProfit`.

Result is one of:
- `TradeResult::Successful` — both legs confirmed, profit realised.
- `TradeResult::FailedSlow` — slow leg failed or timed out, fast leg never submitted.
- `TradeResult::FailedFast` — slow leg OK, fast leg failed (must unwind).

## 6. DB persistence

All writes are fire-and-forget (`FuturesUnordered`) — failures are logged but never fatal:

| What | When | Crate |
|------|------|-------|
| Slow spot prices | On slow block | `kumad/strategy` |
| Fast spot prices | On fast block | `kumad/strategy` |
| Signal | On acceptance (before dedup check) | `kumad/strategy` |
| Trade result | On trade completion | `kumad/execution` |

Signal spot-price foreign keys are resolved at insert time via a SQL CTE — no coordination needed between the spot-price write and the signal write futures.
