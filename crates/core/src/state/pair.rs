use std::{
    collections::HashMap,
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
/// TODO: it isnt normalized to zero2one, its normalized by symbol order
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pair(Token, Token);

impl Pair {
    pub fn new(maybe_a: Token, maybe_b: Token) -> Self {
        if maybe_a.symbol < maybe_b.symbol {
            Self(maybe_a, maybe_b)
        } else {
            Self(maybe_b, maybe_a)
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

    pub fn token_a_b_adjusted_for_usdc(&self) -> (&Token, &Token) {
        if self.token_a().symbol == "USDC" {
            // if A = USDC, B->USDC
            (self.token_b(), self.token_a())
        } else if self.token_b().symbol == "USDC" {
            // if B = USDC, A->USDC
            (self.token_a(), self.token_b())
        } else {
            // else A->B
            (self.token_a(), self.token_b())
        }
    }

    pub fn chain_name(&self) -> tycho_common::models::Chain {
        self.0.chain
    }
}

impl Display for Pair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{}", // ({}, {})",
            self.0.symbol, self.1.symbol,
        )
    }
}

#[derive(Debug, Clone)]
pub struct PairState {
    pub block_height: u64,
    pub states: HashMap<state::PoolId, Arc<dyn ProtocolSim>>,
    pub metadata: HashMap<state::PoolId, Arc<ProtocolComponent>>,
}
