pub(crate) mod builder;

pub(crate) use builder::{
    ArbitrageOpportunity, ArbitrageStrategyBuilder, ArbitrageStrategyHandle, ArbitrageStrategyWorker,
};

#[derive(Debug, Clone)]
pub(crate) struct TradeAmounts {
    pub(crate) amount_in: u64,
    pub(crate) amount_out: u64,
}