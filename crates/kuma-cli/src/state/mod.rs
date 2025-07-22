use std::fmt::Display;

pub mod block;
pub mod pair;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
