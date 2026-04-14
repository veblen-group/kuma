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

/// Human-readable architecture and concept guides.
///
/// These modules contain prose documentation embedded from `docs/` — they are
/// compiled only when building documentation and have no runtime impact.
#[cfg(doc)]
pub mod docs {
    pub mod overview {
        //! What Kuma is and how its components fit together.
        #![doc = include_str!("../../../docs/overview.md")]
    }
    pub mod slow_fast_chain_model {
        //! Why two chains, the 75% timing deadline, and settlement risk.
        #![doc = include_str!("../../../docs/slow-fast-chain-model.md")]
    }
    pub mod signal_lifecycle {
        //! End-to-end: precompute → signal → execution → DB write.
        #![doc = include_str!("../../../docs/signal-lifecycle.md")]
    }
    pub mod signal_profit {
        //! Expected profit: surplus, slippage, congestion, and gas discounts.
        #![doc = include_str!("../../../docs/signal-profit.md")]
    }
    pub mod price_direction {
        //! Spot price convention: USDC as quote, quote-per-base throughout.
        #![doc = include_str!("../../../docs/price-direction.md")]
    }
    pub mod collector {
        //! Block data pipeline: ETH headers, Tycho DEX state, and block multiplexer.
        #![doc = include_str!("../../../docs/collector.md")]
    }
}
