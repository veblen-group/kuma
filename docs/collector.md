# Collector Pipeline

The collector layer ingests real-time on-chain data and assembles it into `Block` objects that strategies can consume.

## Architecture

Each chain runs three parallel workers:

```
ETH collector (eth.rs)
  ├─ subscribes to block headers (WebSocket)         → Header (base fee, block number)
  └─ monitors Transfer events for token balances     → TokenBalances

Tycho collector (tycho.rs)
  └─ subscribes to ProtocolStream (WebSocket)        → BlockSim (pool states)

Block multiplexer (mod.rs)
  ├─ receives Header + TokenBalances from ETH collector
  ├─ receives BlockSim from Tycho collector
  ├─ aligns them by block height (lag tolerance)
  └─ broadcasts Block { header, token_balances, sims }
```

`Block` is broadcast on a `watch` channel — strategies see the latest block, not a queue.

## ETH collector (`crates/core/src/collector/eth.rs`)

Subscribes to `eth_subscribe("newHeads")` via WebSocket. For each new block header:
1. Fetches the latest token balances for the trading account by scanning `Transfer` logs.
2. Sends `(Header, TokenBalances)` downstream.

## Tycho collector (`crates/core/src/collector/tycho.rs`)

Subscribes to Tycho's `ProtocolStreamBuilder` for all registered DEX protocols on the chain:

| Chain | Protocols |
|-------|-----------|
| Ethereum | UniswapV2, SushiswapV2, PancakeswapV2, UniswapV3, PancakeswapV3, UniswapV4 |
| Base | UniswapV2, UniswapV3, UniswapV4 |
| Unichain | UniswapV4 |

TVL filters (`add_tvl_threshold`, `remove_tvl_threshold`) gate which pools are tracked. Pools are added when their TVL exceeds the add threshold and removed when they fall below the remove threshold.

On every `Update` from Tycho, the collector updates `BlockSim` — either initialising from scratch on the first update or applying an incremental patch. Tycho sends a full state snapshot for every pool on every block, so `apply_update` replaces the `states` map wholesale and only patches `metadata` for added/removed pools.

## Block multiplexer (`crates/core/src/collector/mod.rs`)

Waits for both the ETH header and the Tycho block sim for the same block height before emitting a `Block`. A configurable lag tolerance allows the Tycho stream to be slightly behind without stalling. If the lag exceeds the tolerance, the multiplexer logs a warning and drops the block.

## BlockStateStream (`crates/core/src/state/block.rs`)

Each strategy subscribes to the block broadcast via a `BlockStateStream`. This is a `Stream` that:
- Wraps the block `watch::Receiver`.
- Extracts only the pool states relevant to that strategy's pair (and its USDC/ETH auxiliary pairs).
- Emits `BlockState` items ready for consumption by the strategy.

```rust
pub struct BlockState {
    pub pair_state: PairState,          // pools for the primary pair
    pub token_a_usdc_state: Option<PairState>,
    pub token_b_usdc_state: Option<PairState>,
    pub eth_usdc_state: Option<PairState>,
    pub token_a_balance: BigUint,
    pub token_b_balance: BigUint,
    pub base_fee: u64,                  // from block header
}
```

Multiple strategies on the same chain share a single block broadcast — the multiplexer runs once per chain, not once per strategy.
