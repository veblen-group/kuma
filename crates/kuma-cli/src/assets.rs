// TODO: add types for asset, decimals & value (to represent inventory)

use crate::chain::ChainInfo;

struct Token {
    address: String,
    chain: ChainInfo,
    metadata: TokenMetadata,
}

struct TokenMetadata {
    symbol: String,
    decimals: u8,
    transfer_gas: u64,
}
