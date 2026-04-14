//! On-chain state types used throughout the pipeline.
//!
//! | Module | Contents |
//! |--------|----------|
//! | [`block`] | `Block`, `BlockState`, `BlockStateStream` — assembled per-block state for strategies |
//! | [`tycho`] | `BlockSim` — snapshot of all DEX pool states at a block, updated from Tycho |
//! | [`pair`] | `Pair` (static token config), `PairState` (live pool map for a pair) |
//! | [`swap`] | `Swap` — actual on-chain swap amounts parsed from receipt logs |
//! | [`erc20`] | ERC20 `Transfer` event parsing |
//! | [`balances`] | `TokenBalances` — tracked token balances for the trading account |
//!
//! `PoolId` is the canonical pool identifier used as map keys throughout.

use std::fmt::Display;

use serde::{Deserialize, Serialize};

pub mod balances;
pub mod block;
pub mod erc20;
pub mod pair;
pub mod swap;
pub mod tycho;

pub use swap::Swap;
// TODO: maybe some address sanitization?
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PoolId(String);

impl Display for PoolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for PoolId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for PoolId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}
