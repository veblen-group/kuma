//! Core library for the Kuma cross-chain arbitrage bot.
//!
//! This crate contains all shared logic used by the `kumad` daemon and `kuma-backend` API server:
//!
//! - **[`collector`]** — Ingests real-time block data from Ethereum RPC and Tycho DEX streams,
//!   assembles them into [`state::block::Block`] objects, and broadcasts per-chain.
//! - **[`strategy`]** — Cross-chain single-hop arbitrage strategy: precomputes slow chain swap
//!   tables, binary-searches for optimal size on fast chain blocks, produces signals.
//! - **[`signals`]** — Signal types (`CrossChainSingleHop`), profit calculation (`ExpectedProfit`,
//!   `RealizedProfit`), and slippage/gas/congestion discounting.
//! - **[`spot_prices`]** — Spot price extraction from AMM pool states, always expressed as
//!   quote-per-base in USDC terms (see `state::pair::Pair::token_a_b_adjusted_for_usdc`).
//! - **[`encoder`]** — Converts swap simulations into signed Ethereum transactions via the
//!   Tycho Router and Permit2.
//! - **[`trade`]** — Sequential two-leg trade execution (slow chain first, fast chain second).
//! - **[`database`]** — PostgreSQL persistence for spot prices, signals, and trade results.
//! - **[`state`]** — On-chain state types: pools, pairs, balances, block state streams.
//! - **[`config`]** — `kuma.yaml` configuration schema and loader.
//! - **[`chain`]** — Chain metadata, RPC config, and transaction signer.

pub mod chain;
pub mod collector;
pub mod config;
pub mod database;
pub mod encoder;
pub mod signals;
pub mod spot_prices;
pub mod state;
pub mod strategy;
pub mod trade;
