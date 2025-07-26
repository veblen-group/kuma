use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pair {
    pub token_a: Token,
    pub token_b: Token,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Token {
    pub symbol: String,
    pub address: String,
    pub decimals: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairPrice {
    pub pair: Pair,
    pub block_height: u64,
    pub price: String, // BigUint as string
    pub pool_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageSignal {
    pub block_height: u64,
    pub slow_chain: String,
    pub slow_pair: Pair,
    pub slow_pool_id: String,
    pub fast_chain: String,
    pub fast_pair: Pair,
    pub fast_pool_id: String,
    pub slow_swap: SwapInfo,
    pub fast_swap: SwapInfo,
    pub surplus_a: String, // BigUint as string
    pub surplus_b: String, // BigUint as string
    pub expected_profit_a: String, // BigUint as string
    pub expected_profit_b: String, // BigUint as string
    pub max_slippage_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapInfo {
    pub token_in: Token,
    pub token_out: Token,
    pub amount_in: String, // BigUint as string
    pub amount_out: String, // BigUint as string
}
