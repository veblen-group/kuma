pub(crate) mod builder;
pub(crate) mod refactored_builder;

pub(crate) use builder::{
    ArbitrageOpportunity, ArbitrageStrategyBuilder, ArbitrageStrategyHandle, ArbitrageStrategyWorker,
};

pub(crate) use refactored_builder::{
    ArbStrategy, ArbSignal, CrossChainArbitrageStrategy, SlowState, FastState, SlowPrecompute,
    run_arb_task, emit_signal,
};

#[derive(Debug, Clone)]
pub(crate) struct TradeAmounts {
    pub(crate) amount_in: u64,
    pub(crate) amount_out: u64,
}