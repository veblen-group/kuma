---
title: Tycho Integration Guide
description: How Kuma uses Tycho for DEX state streaming, local swap simulation, encoding, and submission.
updated: 2026-04-13
---

# Tycho Integration Guide

This document explains how Kuma integrates with Tycho for DEX state streaming, swap simulation, transaction encoding, and submission — and maps each integration point to the relevant step of the [TAP 6 trade lifecycle](tap6-proposal.md).

## Table of Contents
1. [Overview](#overview)
2. [Crate Dependencies](#crate-dependencies)
3. [Protocol Stream & Block Updates](#3-protocol-stream--block-updates)
4. [Swap Simulation](#4-swap-simulation)
5. [Price Feeds & USDC Conversion](#5-price-feeds--usdc-conversion)
6. [Transaction Encoding](#6-transaction-encoding)
7. [Transaction Submission](#7-transaction-submission)
8. [Configuration](#8-configuration)
9. [Data Flow Summary](#9-data-flow-summary)

---

## Overview

Kuma's integration involves two distinct concerns that should not be conflated:

**Tycho Indexer** — a hosted service (`tycho-beta.propellerheads.xyz`) that indexes DEX pool state in real time and streams it over WebSocket. Kuma connects to it via `ProtocolStreamBuilder` (from `tycho-client`, re-exported by `tycho-simulation`). The Indexer is responsible for data delivery; Kuma never calls it for computation.

**Tycho libraries** — three Rust crates used locally:

| Crate | Role |
|-------|------|
| `tycho-simulation` | Local swap simulation against pool state snapshots (`ProtocolSim` trait); also bundles `tycho-client` for connecting to the Indexer |
| `tycho-execution` | Transaction encoding for the Tycho Router contract (Permit2, ABI encoding, `Solution` → calldata) |
| `tycho-common` | Shared types: `Token`, `Address`, `Bytes`, `Chain` |

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                         Tycho Indexer                           │
│              (hosted service — tycho-beta.propellerheads.xyz)   │
│              streams DEX pool state via WebSocket               │
└──────────────────────────┬──────────────────────────────────────┘
                           │ WebSocket  [tycho-client inside tycho-simulation]
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│            ProtocolStreamBuilder  (tycho-simulation)             │
│  - Registers exchanges (UniV2, V3, V4, Pancake, Sushi)          │
│  - Applies TVL filters                                           │
│  - Streams Update objects per block                              │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                    BlockSim  (kuma-core)                         │
│  - Stores pool states: HashMap<PoolId, Arc<dyn ProtocolSim>>    │
│  - Stores metadata: HashMap<PoolId, Arc<ProtocolComponent>>     │
│  - apply_update(): add/remove pools, replace state snapshot     │
└──────────────────────────┬──────────────────────────────────────┘
                           │  local computation — no network calls
              ┌────────────┴────────────┐
              ▼                         ▼
┌─────────────────────────┐  ┌─────────────────────────┐
│    Swap Simulation      │  │    Spot Prices          │
│  ProtocolSim            │  │  ProtocolSim            │
│    .get_amount_out()    │  │    .spot_price()        │
│  (tycho-simulation)     │  │  (tycho-simulation)     │
└─────────────────────────┘  └─────────────────────────┘
              │                         │
              └────────────┬────────────┘
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Signal Generation  (kuma-core)                │
│  - Precomputes swap tables on slow chain                        │
│  - Binary search for optimal amount on fast chain               │
│  - Calculates expected profit in USDC                            │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│           TychoRouterEncoderBuilder  (tycho-execution)           │
│  - Creates Solution from swap parameters                         │
│  - Encodes calldata for TychoRouter contract                    │
│  - Generates Permit2 signatures                                  │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│              Transaction Execution  (Alloy — not Tycho)          │
│  - estimate_gas(), send_transaction(), get_receipt()            │
│  - Sequential: slow chain first → fast chain on confirmation    │
└─────────────────────────────────────────────────────────────────┘
```

---

## Crate Dependencies

### Cargo.toml Imports

```toml
[dependencies]
tycho-simulation = { version = "0.x" }
tycho-execution  = { version = "0.x" }
tycho-common     = { version = "0.x" }
```

### Rust Import Summary

```rust
// === tycho-simulation: stream client (tycho-client re-export) ===
use tycho_simulation::evm::stream::ProtocolStreamBuilder;
use tycho_simulation::tycho_client::feed::component_tracker::ComponentFilter;
use tycho_simulation::tycho_client::rpc::HttpRPCClient;

// === tycho-simulation: protocol state types ===
use tycho_simulation::evm::protocol::uniswap_v2::state::UniswapV2State;
use tycho_simulation::evm::protocol::uniswap_v3::state::UniswapV3State;
use tycho_simulation::evm::protocol::uniswap_v4::state::UniswapV4State;
use tycho_simulation::evm::protocol::pancakeswap_v2::state::PancakeswapV2State;
use tycho_simulation::protocol::models::{ProtocolComponent, Update};

// === tycho-simulation: local simulation trait ===
use tycho_simulation::tycho_core::simulation::protocol_sim::ProtocolSim;

// === tycho-execution ===
use tycho_execution::encoding::evm::encoder_builders::TychoRouterEncoderBuilder;
use tycho_execution::encoding::evm::approvals::permit2::PermitSingle;
use tycho_execution::encoding::models::{
    EncodedSolution, Solution, Swap as TychoSwap,
    Transaction as TychoTransaction, UserTransferType
};

// === tycho-common ===
use tycho_common::Bytes;
use tycho_common::models::token::Token;
use tycho_common::models::Address;
use tycho_common::models::Chain;
```

---

## 3. Protocol Stream & Block Updates

> *TAP context: this implements [Block Update Ingestion](tap6-proposal.md#block-update-ingestion) — step 1 of the pipeline. The Tycho Indexer streams per-block pool state updates; Kuma multiplexes them per `(chain, token pair)` and feeds them into strategy workers.*

### Overview

`ProtocolStreamBuilder` (from `tycho-client` inside `tycho-simulation`) opens a WebSocket connection to the Tycho Indexer and delivers `Update` objects on every block. These updates are applied to a local `BlockSim` snapshot that strategies query via the `ProtocolSim` trait — no further network calls are needed for simulation.

### Stream Builder Setup

**File: `crates/core/src/collector/tycho.rs`**

```rust
impl Builder {
    pub async fn build(self) -> eyre::Result<(Handle, JoinHandle<()>)> {
        let protocol_stream = ProtocolStreamBuilder::new(
            &self.tycho_url,  // e.g. "tycho-beta.propellerheads.xyz"
            self.chain,       // tycho_models::Chain enum
        );

        // TVL filter: add pool when TVL ≥ add_threshold, remove when < remove_threshold
        let tvl_filter = ComponentFilter::with_tvl_range(
            self.remove_tvl_threshold,  // e.g. 1.0 ($1M)
            self.add_tvl_threshold,     // e.g. 5.0 ($5M)
        );

        // Register DEX protocols per chain
        let protocol_stream = match self.chain {
            Chain::Ethereum => protocol_stream
                .exchange::<UniswapV2State>("uniswap_v2", tvl_filter.clone(), None)
                .exchange::<UniswapV2State>("sushiswap_v2", tvl_filter.clone(), None)
                .exchange::<PancakeswapV2State>("pancakeswap_v2", tvl_filter.clone(), None)
                .exchange::<UniswapV3State>("uniswap_v3", tvl_filter.clone(), None)
                .exchange::<UniswapV3State>("pancakeswap_v3", tvl_filter.clone(), None)
                .exchange::<UniswapV4State>("uniswap_v4", tvl_filter.clone(), None),

            Chain::Base => protocol_stream
                .exchange::<UniswapV2State>("uniswap_v2", tvl_filter.clone(), None)
                .exchange::<UniswapV3State>("uniswap_v3", tvl_filter.clone(), None)
                .exchange::<UniswapV4State>("uniswap_v4", tvl_filter.clone(), None),

            Chain::Unichain => protocol_stream
                .exchange::<UniswapV4State>("uniswap_v4", tvl_filter.clone(), None),

            _ => return Err(eyre!("Unsupported chain")),
        };

        let protocol_stream = protocol_stream
            .auth_key(Some(self.tycho_api_key))
            .skip_state_decode_failures(true)
            .set_tokens(token_addresses);  // Filter to tokens we care about

        let mut stream = protocol_stream.build().await?;
        // ... spawn worker task
    }
}
```

### Processing Block Updates

**File: `crates/core/src/collector/tycho.rs`**

```rust
async fn worker(
    mut protocol_stream: impl Stream<Item = Update> + Unpin,
    block_sim_tx: watch::Sender<Arc<Option<Block>>>,
) {
    let mut curr_block_sim: Option<BlockSim> = None;

    while let Some(block_update) = protocol_stream.next().await {
        curr_block_sim = Some(match curr_block_sim.take() {
            None => BlockSim::new(block_update),                  // first block
            Some(mut sim) => { sim.apply_update(block_update); sim }  // incremental
        });
        let _ = block_sim_tx.send(Arc::new(curr_block_sim.clone()));
    }
}
```

### BlockSim State Management

**File: `crates/core/src/state/tycho.rs`**

`BlockSim` is Kuma's local mirror of the Tycho Indexer's pool state. Tycho sends a full `states` snapshot on every block, so `apply_update` replaces it wholesale and only patches `metadata` for added/removed pools:

```rust
pub struct BlockSim {
    pub block_number_or_timestamp: u64,
    states: HashMap<PoolId, Arc<dyn ProtocolSim>>,    // simulation state
    metadata: HashMap<PoolId, Arc<ProtocolComponent>>, // encoding metadata
}

impl BlockSim {
    pub fn apply_update(&mut self, update: Update) {
        self.block_number_or_timestamp = update.block_number_or_timestamp;
        for (id, _) in update.removed_pairs { self.states.remove(...); self.metadata.remove(...); }
        for (id, c) in update.new_pairs { self.metadata.insert(...); }
        for (id, s) in update.states { self.states.insert(...); }  // full replacement
    }
}
```

### Update Structure (from Tycho Indexer)

```rust
pub struct Update {
    pub block_number_or_timestamp: u64,
    pub states: HashMap<String, Box<dyn ProtocolSim>>,   // full snapshot each block
    pub new_pairs: HashMap<String, ProtocolComponent>,   // pools added (TVL crossed up)
    pub removed_pairs: HashMap<String, ProtocolComponent>, // pools removed (TVL crossed down)
}
```

---

## 4. Swap Simulation

> *TAP context: this covers two steps of [Signal Generation](tap6-proposal.md#signal-generation):*
> - *Step 1b (slow block): precompute `amount_in/amount_out` tables for the slow chain across `binary_search_steps` inventory points.*
> - *Step 2a (fast block): binary search over the precomputed table using live fast-chain simulation to find the direction and size that maximises surplus.*

### Overview

`ProtocolSim::get_amount_out` is called locally against in-memory pool state — no network call is made. All simulation happens against the `BlockSim` snapshot delivered by the Tycho Indexer.

### ProtocolSim Trait (from `tycho-simulation`)

```rust
pub trait ProtocolSim {
    fn get_amount_out(
        &self,
        amount_in: BigUint,
        token_in: &Token,
        token_out: &Token,
    ) -> Result<SimResult>;  // SimResult { amount: BigUint, gas: u64 }

    fn spot_price(&self, base: &Token, quote: &Token) -> Result<f64>;
}
```

### Precomputing Swap Tables (slow chain, on each slow block)

**File: `crates/core/src/strategy/simulation.rs`**

```rust
impl PoolSteps {
    pub fn from_protocol_sim(
        pair: &Pair,
        binary_search_steps: usize,
        inventory: &(BigUint, BigUint),
        protocol_sim: &dyn ProtocolSim,
    ) -> eyre::Result<Self> {
        let a_to_b = Self::for_direction(pair.token_a(), pair.token_b(), &inventory.0, binary_search_steps, protocol_sim)?;
        let b_to_a = Self::for_direction(pair.token_b(), pair.token_a(), &inventory.1, binary_search_steps, protocol_sim)?;
        Ok(Self { a_to_b, b_to_a })
    }

    fn for_direction(token_in: &Token, token_out: &Token, max_amount: &BigUint, steps: usize, protocol_sim: &dyn ProtocolSim) -> eyre::Result<Vec<Swap>> {
        (0..steps).filter_map(|step| {
            let amount_in = max_amount * (step + 1) / steps;
            Swap::from_protocol_sim(&amount_in, token_in, token_out, protocol_sim).ok()
        }).collect()
    }
}
```

### Binary Search for Optimal Amount (fast chain, on each fast block)

**File: `crates/core/src/strategy/mod.rs`**

```rust
fn find_optimal_signal(
    &self,
    slow_sims: &[Swap],           // precomputed slow chain table
    fast_state: &dyn ProtocolSim, // live fast chain state (no network call)
) -> Option<signals::CrossChainSingleHop> {
    let (mut left, mut right) = (0, slow_sims.len() - 1);
    let mut best_signal = None;

    while left < right {
        let mid = (right + left) / 2;
        let slow_swap = &slow_sims[mid];

        // Simulate fast chain using slow output as input
        let fast_swap = self.swap_from_precompute(slow_swap.clone(), fast_state, self.max_slippage_bps)?;

        let mid_signal = signals::CrossChainSingleHop::try_from_simulations(&slow_swap, &fast_swap, ...)?;

        if mid_profit < next_profit { left = mid + 1; best_signal = Some(next_signal); }
        else { right = mid; }
    }
    best_signal
}
```

---

## 5. Price Feeds & USDC Conversion

> *TAP context: this covers [Signal Generation](tap6-proposal.md#signal-generation) step 2b — converting the raw token surplus to a USDC-denominated expected profit, and deducting gas cost (also in USDC). Spot prices are fetched via the same local `ProtocolSim` instances — no external price oracle is used.*

### Overview

Spot prices are extracted from pool states using `ProtocolSim::spot_price(base, quote)`, which returns **quote per base** entirely from local state. USDC is always the quote token — see [price-direction.md](price-direction.md) for the full explanation.

### Spot Price Extraction

**File: `crates/core/src/spot_prices.rs`**

```rust
pub fn try_make_sorted_spot_prices(state: &PairState, pair: &Pair) -> eyre::Result<Vec<(PoolId, f64)>> {
    let mut spot_prices: Vec<(PoolId, f64)> = state.states.iter()
        .filter_map(|(id, pool)| {
            let (base, quote) = pair.token_a_b_adjusted_for_usdc(); // USDC always quote
            pool.spot_price(base, quote).ok().map(|p| (id.clone(), p))
        })
        .collect();
    spot_prices.sort_by(|(_, a), (_, b)| a.total_cmp(b));
    Ok(spot_prices)
}
```

### USDC Conversion for Profit Calculation

**File: `crates/core/src/strategy/mod.rs`** — `try_precompute` fetches four price feeds on each slow block:

```rust
pub fn try_precompute(&self, state: BlockState) -> eyre::Result<Precomputes> {
    let prices_a_b     = SpotPrices::try_from_pair_state(&state.pair_state,       self.slow_pair.clone(),      ...)?;
    let prices_a_usdc  = SpotPrices::try_from_pair_state(&state.token_a_usdc_state, token_a_usdc_pair.clone(), ...)?; // None if A is USDC
    let prices_b_usdc  = SpotPrices::try_from_pair_state(&state.token_b_usdc_state, token_b_usdc_pair.clone(), ...)?; // None if B is USDC
    let prices_eth_usdc = SpotPrices::try_from_pair_state(&state.eth_usdc_state,   eth_usdc_pair.clone(),      ...)?; // for gas cost
    Ok(Precomputes { prices_a_b, prices_a_usdc, prices_b_usdc, prices_eth_usdc, .. })
}
```

Pessimistic (`min`) prices are used for conservative profit estimates:

```rust
pub fn try_mul_amount_usdc_price(amount: &BigUint, prices: &Option<SpotPrices>) -> eyre::Result<(BigUint, f64)> {
    match prices {
        Some(sp) => Ok((try_mul_biguint_f64(amount, sp.min_price, &sp.pair)?, sp.min_price)),
        None => Ok((amount.clone(), 1.0)),  // token is already USDC
    }
}
```

---

## 6. Transaction Encoding

> *TAP context: this covers [Trade Execution — Encoding Calldata](tap6-proposal.md#encoding-calldata--account-management). Calldata is encoded at signal generation time (not at promotion time) so it's ready immediately when the signal is promoted. The nonce and Permit2 signature are attached at promotion.*

### Overview

`tycho-execution` converts swap parameters into ABI-encoded calldata for the [Tycho Router](https://github.com/propeller-heads/tycho-execution/blob/main/foundry/src/TychoRouter.sol) using Permit2 for token approvals.

### Creating a Solution

**File: `crates/core/src/encoder.rs`**

```rust
pub(crate) fn create_solution(component: ProtocolComponent, swap: &Swap, signer: PrivateKeySigner) -> eyre::Result<Solution> {
    let tycho_swap = TychoSwap::new(
        component,                     // ProtocolComponent from BlockSim.metadata
        swap.token_in.address.clone(),
        swap.token_out.address.clone(),
        0f64,                          // split = 0 means 100% of given_amount
        None, None, None,
    );
    Ok(Solution {
        sender: signer_address.clone(),
        receiver: signer_address.clone(),
        given_token: swap.token_in.address.clone(),
        given_amount: swap.amount_in.clone(),
        checked_token: swap.token_out.address.clone(),
        checked_amount: swap.amount_out.clone(),  // minimum expected output
        exact_out: false,
        swaps: vec![tycho_swap],
        native_action: None,
    })
}
```

### Encoding the Solution

```rust
pub(crate) fn encode_solution(solution: Solution, chain: &Chain) -> eyre::Result<EncodedSolution> {
    let encoder = TychoRouterEncoderBuilder::new()
        .chain(chain.name)
        .user_transfer_type(UserTransferType::TransferFromPermit2)
        .build()?;
    let encoded = encoder.encode_solutions(vec![solution])?;
    Ok(encoded[0].clone())
}
```

### Permit2 Signing

```rust
fn sign_permit(chain_id: u64, permit_single: &models::PermitSingle, signer: PrivateKeySigner) -> Result<Signature, EncodingError> {
    let permit2_address = alloyAddress::from_str("0x000000000022D473030F116dDEE9F6B43aC78BA3")?;
    let domain = eip712_domain! { name: "Permit2", chain_id: chain_id, verifying_contract: permit2_address };
    let permit = PermitSingle::try_from(permit_single)?;
    signer.sign_hash_sync(&permit.eip712_signing_hash(&domain))
        .map_err(|e| EncodingError::FatalError(format!("Sign failed: {e}")))
}
```

### Complete Encoding Flow

```
Swap (amount_in, amount_out, tokens)
        │
        ▼
create_solution()  ──►  Solution { sender, receiver, given_token/amount, checked_token/amount, swaps }
        │
        ▼
encode_solution()  ──►  EncodedSolution { permit: PermitSingle, function_signature, interacting_with, swaps: bytes }
        │
        ▼
encode_tycho_router_call()  ──►  TychoTransaction { to: TychoRouter, value, data: ABI calldata }
        │
        ▼
UnsignedTransaction (Alloy TransactionRequest)
```

---

## 7. Transaction Submission

> *TAP context: this is the execution phase of [Trade Execution](tap6-proposal.md#trade-execution). Submission uses Alloy directly — Tycho is not involved. Sequential execution (slow chain first) is the key design choice for minimising settlement risk from mismatched block times.*

### Overview

Transaction submission uses Alloy, not Tycho. The key design: submit slow chain first and wait for confirmation before submitting the fast chain leg.

### Sequential Trade Execution

**File: `crates/core/src/trade.rs`**

```rust
impl Trade {
    pub async fn run(self) -> eyre::Result<TradeResult> {
        // 1. Execute slow chain — if this fails, fast leg is never submitted
        let slow_receipt = execute_tx(&self.slow_tx_req, &self.signal.slow_chain).await?;
        if !slow_receipt.status() {
            return Ok(TradeResult::FailedSlow(...));
        }

        // 2. Execute fast chain only after slow confirmation
        let fast_receipt = execute_tx(&self.fast_tx_req, &self.signal.fast_chain).await?;
        if !fast_receipt.status() {
            return Ok(TradeResult::FailedFast(...));  // position must be unwound
        }

        // 3. Parse Transfer logs → RealizedProfit
        Ok(TradeResult::Successful(...))
    }
}
```

### Gas Estimation

```rust
pub async fn estimate_gas_amount(transaction: UnsignedTransaction, chain: &Chain) -> eyre::Result<u64> {
    let provider = alloy::providers::ProviderBuilder::new()
        .wallet(EthereumWallet::new(chain.signer()))
        .connect_http(chain.rpc_url.parse()?);
    provider.estimate_gas(transaction.tx).await
}
```

---

## 8. Configuration

> *TAP context: TVL thresholds and `tycho_api_key` control which pools are tracked (Block Update Ingestion); `binary_search_steps`, `max_slippage_bps`, and `congestion_risk_discount_bps` shape Signal Generation and profit discounting.*

```yaml
# kuma.yaml
tycho_api_key: "your-api-key"

chains:
  - name: ethereum
    tycho_url: "tycho-beta.propellerheads.xyz"
    add_tvl_threshold: 5.0      # add pool when TVL ≥ $5M
    remove_tvl_threshold: 1.0   # remove pool when TVL < $1M

binary_search_steps: 20         # inventory points precomputed per pool per slow block
max_slippage_bps: 50            # 0.5% — applied to amount_out on both legs
congestion_risk_discount_bps: 200  # 2% flat discount on surplus
```

Environment variable overrides (Figment, `KUMA_` prefix, `__` separator):

```bash
KUMA_TYCHO_API_KEY=your-api-key
KUMA_CHAINS__0__TYCHO_URL=tycho-beta.propellerheads.xyz
```

---

## 9. Data Flow Summary

Maps the complete trade lifecycle to Tycho integration points:

```
TAP Step                    Tycho touchpoint
─────────────────────────────────────────────────────────────────
Block Update Ingestion
  WebSocket stream      →   ProtocolStreamBuilder (tycho-client)
  Pool state update     →   BlockSim.apply_update() (ProtocolSim instances)

Signal Generation — slow block
  Precompute swap table  →  ProtocolSim.get_amount_out() × binary_search_steps
  Spot prices            →  ProtocolSim.spot_price() × 4 pairs

Signal Generation — fast block
  Binary search          →  ProtocolSim.get_amount_out() (live fast state)
  USDC profit calc       →  SpotPrices (from slow precompute, no new calls)

Trade Execution — encoding (at signal generation time)
  Encode calldata        →  TychoRouterEncoderBuilder (tycho-execution)
  Sign Permit2           →  PermitSingle EIP712 (tycho-execution)

Trade Execution — submission (at promotion time)
  submit slow tx         →  Alloy provider (NOT Tycho)
  submit fast tx         →  Alloy provider (NOT Tycho)
```

### Key Tycho Methods

| Method | Crate | Purpose | Kuma location |
|--------|-------|---------|---------------|
| `ProtocolStreamBuilder::new()` | `tycho-simulation` (tycho-client) | Open WebSocket to Indexer | `collector/tycho.rs` |
| `.exchange::<State>()` | `tycho-simulation` | Register DEX protocol | `collector/tycho.rs` |
| `stream.next()` | `tycho-simulation` | Receive `Update` per block | `collector/tycho.rs` |
| `pool.get_amount_out()` | `tycho-simulation` | Simulate swap locally | `strategy/simulation.rs` |
| `pool.spot_price()` | `tycho-simulation` | Get mid-price locally | `spot_prices.rs` |
| `TychoRouterEncoderBuilder::new()` | `tycho-execution` | Build calldata encoder | `encoder.rs` |
| `encoder.encode_solutions()` | `tycho-execution` | Encode + Permit2 | `encoder.rs` |
