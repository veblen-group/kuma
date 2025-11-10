use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    sync::Arc,
};

use crate::state::{self};
use serde::{Deserialize, Serialize};
use tycho_simulation::{
    protocol::models::ProtocolComponent, tycho_common::models::token::Token,
    tycho_core::simulation::protocol_sim::ProtocolSim,
};

/// Represents a pair of tokens, normalized to Uniswap's zero2one direction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pair(Token, Token);

impl Pair {
    pub fn new(token_a: Token, token_b: Token) -> Self {
        if token_a.symbol < token_b.symbol {
            Self(token_a, token_b)
        } else {
            Self(token_b, token_a)
        }
    }

    pub fn in_token_vec(&self, tokens: &[Token]) -> bool {
        tokens.contains(&self.0) && tokens.contains(&self.1)
    }

    pub fn token_a(&self) -> &Token {
        &self.0
    }

    pub fn token_b(&self) -> &Token {
        &self.1
    }
}

impl Display for Pair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{}", // ({}, {})",
            self.0.symbol,
            self.1.symbol, // self.0.address, self.1.address
        )
    }
}

#[derive(Debug, Clone)]
pub struct PairState {
    pub block_height: u64,
    pub states: HashMap<state::PoolId, Arc<dyn ProtocolSim>>,
    pub modified_pools: Arc<HashSet<state::PoolId>>,
    pub unmodified_pools: Arc<HashSet<state::PoolId>>,
    pub metadata: HashMap<state::PoolId, Arc<ProtocolComponent>>,
}
