---
title: Slow/Fast Chain Model
description: The two-chain architecture — rationale, 75% timing deadline, and settlement risk.
updated: 2026-04-13
---

# Slow/Fast Chain Model

Kuma's strategy is built around a pair of chains with different block times.

## The asymmetry

| Role | Chain (typical) | Block time | Why |
|------|-----------------|------------|-----|
| **Slow** | Ethereum mainnet | ~12 s | Longer block time means the price we lock in on this chain stays valid longer, giving us a window to observe the fast chain |
| **Fast** | Base / Unichain | ~2 s | Many fast blocks happen per slow block; we pick the best one to trade against |

Because the fast chain produces many blocks per slow block, the strategy can refresh its view of the fast chain price multiple times before it has to commit. The slow chain price is what we "lock in" — once we submit there, we're committed.

## The timing loop

```
slow block N arrives
  └─ precompute swap tables for slow chain
  └─ start 75% timer  ──────────────────────────────────────────────────┐
                                                                         │
fast block arrives (possibly multiple)                                   │
  └─ simulate fast chain                                                  │
  └─ generate or update curr_signal                                       │
                                                                         │
75% of slow block time elapses ──────────────────────────────────────────┘
  └─ emit curr_signal to execution
```

The **75% deadline** (`slow_block_time * 0.75`) is the emission trigger. Waiting until 75% of the block time ensures the fast chain data is as fresh as possible, while still leaving headroom to submit the slow chain transaction before the next block.

See `crates/kumad/src/strategy/mod.rs` for the event loop implementation.

## In config

```yaml
strategies:
  - token_a: USDC
    token_b: WETH
    slow_chain: ethereum   # long block time — precompute here
    fast_chain: unichain   # short block time — react here
```

## Settlement risk rationale

Executing the slow chain leg first and waiting for confirmation before submitting the fast chain leg means:
- If the slow leg fails (e.g. slippage, congestion) → fast leg is never submitted, no open position.
- If the fast leg fails after the slow leg succeeded → we must unwind on the slow chain. Sequential order minimises the chance of this happening.

See the [signal-lifecycle](signal-lifecycle.md) for the full execution flow, and [tap6-proposal.md](tap6-proposal.md) §Congestion & Settlement Risk for the mathematical treatment.
