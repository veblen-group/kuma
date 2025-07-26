pub mod chain;
pub mod collector;
pub mod config;
pub mod signals;
pub mod state;
pub mod strategy;

// pub use chain::Chain;
// pub use config::*;
// pub use state::{PoolId, block::Block};

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
