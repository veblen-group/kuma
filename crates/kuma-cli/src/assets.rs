// TODO: add types for asset, decimals & value (to represent inventory)

use crate::chain::Chain;

struct Token {
    address: String,
    chain: Chain,
    metadata: TokenMetadata,
}

struct TokenMetadata {
    symbol: String,
    decimals: u8,
    transfer_gas: u64,
}
