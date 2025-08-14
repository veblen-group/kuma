use std::fmt::Display;

use serde::{Deserialize, Serialize};

pub mod balances;
pub mod block;
pub mod pair;

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
