# Kuma — Cross-Chain Arbitrage Bot

Kuma is an automated cross-chain arbitrage bot that exploits price discrepancies in AMM pools across EVM-compatible blockchains. It monitors DEX pool states in real time, identifies profitable arbitrage opportunities, and executes two-legged trades sequentially to capture the surplus.

![High-level architecture](../../high-level.png)

## How it works

1. **Ingest** — Real-time DEX pool state is streamed from the [Tycho Indexer](https://docs.propellerheads.xyz/tycho/for-solvers/simulation) via WebSocket. One stream per `(chain, token pair)`.
2. **Signal generation** — A strategy monitors a pair of chains. When the slow chain's block arrives it precomputes swap tables; when the fast chain's block arrives it detects a price discrepancy, finds the optimal trade size, discounts for risk, and emits a signal.
3. **Trade execution** — A signal is promoted to a trade: the slow chain leg is submitted first, and only on confirmation is the fast chain leg submitted. This sequential order minimises settlement risk.

## Crate map

| Crate | Role | Docs |
|-------|------|------|
| `kuma-core` | All shared logic: collectors, strategy, signals, encoding, database | [kuma_core](../../index.html) |
| `kumad` | The main binary: wires everything together, runs the event loops | [kumad](../../../kumad/index.html) |
| `kuma-backend` | HTTP API for monitoring (spot prices, signals, trades) | [kuma_backend](../../../kuma_backend/index.html) |
| `kuma-cli` | Utilities: generate a single signal, list tokens, init Permit2 | [kuma_cli](../../../kuma_cli/index.html) |

## Key docs

- [Slow/fast chain model][crate::docs::slow_fast_chain_model]
- [Signal lifecycle][crate::docs::signal_lifecycle]
- [Price direction & spot prices][crate::docs::price_direction]
- [Expected profit & discounting][crate::docs::signal_profit]
- [Collector pipeline][crate::docs::collector]
- `tycho-integration.md` — Tycho indexer integration notes (source repo)
- `docs/DEPLOYMENT.md` — GCP deployment guide (source repo)

## Original design doc

See `tap6-proposal.md` in the source repository for the full mathematical treatment of surplus calculation, profit discounting, and trade execution design.
