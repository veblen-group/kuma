# Tycho Library Integration Guide

This document provides extensive documentation on how the Kuma cross-chain arbitrage bot integrates with Tycho's libraries for swap simulation, USD price feeds, transaction encoding, and submission.

## Table of Contents
1. [Overview](#overview)
2. [Crate Dependencies](#crate-dependencies)
3. [Swap Simulation](#1-swap-simulation)
4. [Price Feeds & USDC Conversion](#2-price-feeds--usdc-conversion)
5. [Transaction Encoding](#3-transaction-encoding)
6. [Protocol Stream & Block Updates](#4-protocol-stream--block-updates)
7. [Transaction Submission](#5-transaction-submission)
8. [Configuration](#6-configuration)
9. [Data Flow Summary](#7-data-flow-summary)

---

## Overview

Kuma uses three main Tycho crates:
- **`tycho_simulation`** - Real-time DEX pool state streaming and swap simulation
- **`tycho_execution`** - Transaction encoding for the Tycho Router contract
- **`tycho_common`** - Shared types (Token, Address, Bytes, Chain)

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                         Tycho Indexer                           │
│                    (tycho-beta.propellerheads.xyz)              │
└──────────────────────────┬──────────────────────────────────────┘
                           │ WebSocket
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                    ProtocolStreamBuilder                         │
│  - Registers exchanges (UniV2, V3, V4, Pancake, Sushi)          │
│  - Applies TVL filters                                           │
│  - Streams BlockUpdate objects                                   │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                         BlockSim                                 │
│  - Stores pool states: HashMap<PoolId, Arc<dyn ProtocolSim>>    │
│  - Stores metadata: HashMap<PoolId, Arc<ProtocolComponent>>     │
│  - Processes: new_pairs, removed_pairs, updated_states          │
└──────────────────────────┬──────────────────────────────────────┘
                           │
              ┌────────────┴────────────┐
              ▼                         ▼
┌─────────────────────────┐  ┌─────────────────────────┐
│    Swap Simulation      │  │    Spot Prices          │
│  protocol_sim           │  │  protocol_sim           │
│    .get_amount_out()    │  │    .spot_price()        │
└─────────────────────────┘  └─────────────────────────┘
              │                         │
              └────────────┬────────────┘
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Signal Generation                             │
│  - Precomputes swap tables on slow chain                        │
│  - Binary search for optimal amount                              │
│  - Calculates expected profit in USDC                            │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                  TychoRouterEncoderBuilder                       │
│  - Creates Solution from swap parameters                         │
│  - Encodes calldata for TychoRouter contract                    │
│  - Generates Permit2 signatures                                  │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Transaction Execution                         │
│  - Alloy provider: estimate_gas(), send_transaction()           │
│  - Sequential: slow chain → fast chain                          │
└─────────────────────────────────────────────────────────────────┘
```

---

## Crate Dependencies

### Cargo.toml Imports

```toml
[dependencies]
tycho-simulation = { version = "0.x" }
tycho-execution = { version = "0.x" }
tycho-common = { version = "0.x" }
```

### Rust Import Summary

```rust
// === tycho_simulation crate ===
use tycho_simulation::evm::stream::ProtocolStreamBuilder;
use tycho_simulation::evm::protocol::uniswap_v2::state::UniswapV2State;
use tycho_simulation::evm::protocol::uniswap_v3::state::UniswapV3State;
use tycho_simulation::evm::protocol::uniswap_v4::state::UniswapV4State;
use tycho_simulation::evm::protocol::pancakeswap_v2::state::PancakeswapV2State;
use tycho_simulation::protocol::models::{ProtocolComponent, Update};
use tycho_simulation::tycho_core::simulation::protocol_sim::ProtocolSim;
use tycho_simulation::tycho_client::feed::component_tracker::ComponentFilter;
use tycho_simulation::tycho_client::rpc::HttpRPCClient;

// === tycho_execution crate ===
use tycho_execution::encoding::evm::encoder_builders::TychoRouterEncoderBuilder;
use tycho_execution::encoding::evm::approvals::permit2::PermitSingle;
use tycho_execution::encoding::models::{
    EncodedSolution, Solution, Swap as TychoSwap,
    Transaction as TychoTransaction, UserTransferType
};

// === tycho_common crate ===
use tycho_common::Bytes;
use tycho_common::models::token::Token;
use tycho_common::models::Address;
use tycho_common::models::Chain;
```

---

## 1. Swap Simulation

### Overview

Swap simulation uses Tycho's `ProtocolSim` trait to calculate exact output amounts for given inputs without executing on-chain. This is critical for:
- Finding optimal trade amounts via binary search
- Estimating slippage and expected profit
- Precomputing swap tables for fast signal generation

### Key Files

| File | Purpose |
|------|---------|
| `crates/core/src/strategy/simulation.rs` | Swap simulation helpers |
| `crates/core/src/state/tycho.rs` | BlockSim state management |
| `crates/core/src/strategy/mod.rs` | Strategy precomputation |

### ProtocolSim Trait

The `ProtocolSim` trait (from `tycho_simulation`) provides:

```rust
pub trait ProtocolSim {
    /// Calculate output amount for a given input
    fn get_amount_out(
        &self,
        amount_in: BigUint,
        token_in: &Token,
        token_out: &Token,
    ) -> Result<SimResult>;  // SimResult { amount: BigUint, gas: u64 }

    /// Get spot price (no slippage)
    fn spot_price(
        &self,
        token_a: &Token,
        token_b: &Token,
    ) -> Result<f64>;
}
```

### Swap Simulation Implementation

**File: `crates/core/src/strategy/simulation.rs:29-47`**

```rust
impl Swap {
    /// Simulate a swap using a Tycho ProtocolSim
    pub fn from_protocol_sim(
        amount_in: &BigUint,
        token_in: &Token,
        token_out: &Token,
        protocol_sim: &dyn ProtocolSim,
    ) -> eyre::Result<Self> {
        // Call Tycho's get_amount_out() method
        let sim_result = protocol_sim
            .get_amount_out(amount_in.clone(), token_in, token_out)
            .wrap_err("simulation failed")?;

        Ok(Self {
            token_in: token_in.clone(),
            amount_in: amount_in.clone(),
            token_out: token_out.clone(),
            amount_out: sim_result.amount,  // Simulated output
        })
    }
}
```

### Precomputing Swap Tables

The strategy precomputes swap simulations at multiple amounts for efficient signal generation:

**File: `crates/core/src/strategy/simulation.rs:76-132`**

```rust
impl PoolSteps {
    /// Create swap simulations at exponentially spaced amounts
    pub fn from_protocol_sim(
        pair: &Pair,
        binary_search_steps: usize,
        inventory: &(BigUint, BigUint),
        protocol_sim: &dyn ProtocolSim,
    ) -> eyre::Result<Self> {
        let a_to_b = Self::for_direction(
            pair.token_a(),
            pair.token_b(),
            &inventory.0,  // Available token_a
            binary_search_steps,
            protocol_sim,
        )?;

        let b_to_a = Self::for_direction(
            pair.token_b(),
            pair.token_a(),
            &inventory.1,  // Available token_b
            binary_search_steps,
            protocol_sim,
        )?;

        Ok(Self { a_to_b, b_to_a })
    }

    /// Simulate swaps at exponentially increasing amounts
    fn for_direction(
        token_in: &Token,
        token_out: &Token,
        max_amount: &BigUint,
        steps: usize,
        protocol_sim: &dyn ProtocolSim,
    ) -> eyre::Result<Vec<Swap>> {
        let mut swaps = Vec::with_capacity(steps);

        for step in 0..steps {
            // Exponential spacing: amount = max * (step+1) / steps
            let amount_in = max_amount * (step + 1) / steps;

            match Swap::from_protocol_sim(&amount_in, token_in, token_out, protocol_sim) {
                Ok(swap) => swaps.push(swap),
                Err(_) => break,  // Pool can't handle this size
            }
        }

        Ok(swaps)
    }
}
```

### Binary Search for Optimal Amount

**File: `crates/core/src/strategy/mod.rs:372-506`**

```rust
fn find_optimal_signal(
    &self,
    slow_sims: &[Swap],              // Precomputed slow chain swaps
    fast_state: &dyn ProtocolSim,    // Live fast chain state
    // ... other params
) -> Option<signals::CrossChainSingleHop> {
    let (mut left, mut right) = (0, slow_sims.len() - 1);
    let mut best_signal = None;

    while left < right {
        let mid = (right + left) / 2;

        // Get precomputed slow chain swap
        let slow_swap = &slow_sims[mid];

        // Simulate fast chain swap using slow output as input
        let fast_swap = self.swap_from_precompute(
            slow_swap.clone(),
            fast_state,  // Live ProtocolSim
            fast_inventory,
            self.max_slippage_bps,
        )?;

        // Calculate expected profit
        let mid_signal = signals::CrossChainSingleHop::try_from_simulations(
            &slow_swap, &fast_swap, // ... prices, discounts
        )?;

        // Compare mid vs mid+1 to find peak profit
        if mid_profit < next_profit {
            left = mid + 1;
            best_signal = Some(next_signal);
        } else {
            right = mid;
        }
    }

    best_signal
}
```

---

## 2. Price Feeds & USDC Conversion

### Overview

Spot prices are extracted from pool states to:
- Identify arbitrage opportunities (price discrepancies)
- Convert token amounts to USDC for profit calculation
- Track ETH-USDC for gas cost estimation

### Spot Price Extraction

**File: `crates/core/src/spot_prices.rs:133-164`**

```rust
/// Extract and sort spot prices from all pools
pub fn try_make_sorted_spot_prices(
    state: &PairState,
    pair: &Pair,
) -> eyre::Result<Vec<(PoolId, f64)>> {
    let mut spot_prices: Vec<(PoolId, f64)> = state
        .states
        .iter()
        .filter_map(|(id, pool)| {
            let (token_a, token_b) = pair.token_a_b_adjusted_for_usdc();

            // Call Tycho's spot_price() method
            match pool.spot_price(token_a, token_b) {
                Ok(price) => Some((id.clone(), price)),
                Err(err) => {
                    warn!(error = %err, "failed to get spot price, skipping");
                    None
                }
            }
        })
        .collect();

    // Sort ascending to find min/max
    spot_prices.sort_by(|(_, a), (_, b)| a.total_cmp(b));
    Ok(spot_prices)
}
```

### SpotPrices Structure

```rust
pub struct SpotPrices {
    pub pair: Pair,
    pub block_height: u64,
    pub min_price: f64,      // Best price to buy
    pub max_price: f64,      // Best price to sell
    pub min_pool_id: PoolId,
    pub max_pool_id: PoolId,
    pub chain: Chain,
}
```

### USDC Conversion for Profit Calculation

**File: `crates/core/src/strategy/mod.rs:138-235`**

During precomputation, the strategy calculates multiple price feeds:

```rust
pub fn try_precompute(&self, state: BlockState) -> eyre::Result<Precomputes> {
    // 1. Primary pair prices (e.g., PEPE/WETH)
    let prices_a_b = SpotPrices::try_from_pair_state(
        &state.pair_state,
        self.slow_pair.clone(),
        self.slow_chain.clone(),
    )?;

    // 2. Token A to USDC (if A != USDC)
    let prices_a_usdc = if let Some(token_a_usdc_pair) = &self.slow_token_a_usdc {
        Some(SpotPrices::try_from_pair_state(
            &state.token_a_usdc_state.unwrap(),
            token_a_usdc_pair.clone(),
            self.slow_chain.clone(),
        )?)
    } else {
        None  // Token A is USDC
    };

    // 3. Token B to USDC (if B != USDC)
    let prices_b_usdc = if let Some(token_b_usdc_pair) = &self.slow_token_b_usdc {
        Some(SpotPrices::try_from_pair_state(
            &state.token_b_usdc_state.unwrap(),
            token_b_usdc_pair.clone(),
            self.slow_chain.clone(),
        )?)
    } else {
        None  // Token B is USDC
    };

    // 4. ETH to USDC for gas cost calculation
    let prices_eth_usdc = if let Some(eth_usdc_pair) = &self.slow_eth_usdc {
        Some(SpotPrices::try_from_pair_state(
            &state.eth_usdc_state.unwrap(),
            eth_usdc_pair.clone(),
            self.slow_chain.clone(),
        )?)
    } else {
        None
    };

    Ok(Precomputes {
        prices_a_b,
        prices_a_usdc,
        prices_b_usdc,
        prices_eth_usdc,
        // ...
    })
}
```

### Using Prices for Profit Calculation

**File: `crates/core/src/signals/profit.rs:380-408`**

```rust
/// Convert token amount to USDC using spot prices
pub fn try_mul_amount_usdc_price(
    amount: &BigUint,
    prices: &Option<SpotPrices>,
) -> eyre::Result<(BigUint, f64)> {
    match prices {
        Some(spot_prices) => {
            // Use pessimistic (min) price for conservative estimate
            let price = spot_prices.min_price;
            let usdc_amount = try_mul_biguint_f64(amount, price, &spot_prices.pair)?;
            Ok((usdc_amount, price))
        }
        None => {
            // Token is already USDC
            Ok((amount.clone(), 1.0))
        }
    }
}
```

---

## 3. Transaction Encoding

### Overview

Transaction encoding converts swap parameters into calldata for the Tycho Router contract using:
- `TychoRouterEncoderBuilder` - Builds the encoder
- `Solution` - Represents a complete swap intent
- `EncodedSolution` - ABI-encoded calldata with Permit2

### Creating a Solution

**File: `crates/core/src/encoder.rs:200-232`**

```rust
pub(crate) fn create_solution(
    component: ProtocolComponent,  // Pool metadata from Tycho
    swap: &Swap,                   // Token in/out, amounts
    signer: PrivateKeySigner,
) -> eyre::Result<Solution> {
    let signer_address = tycho_common::models::Address::from_str(
        signer.address().to_string().as_str()
    )?;

    // Create Tycho Swap object
    let tycho_swap = TychoSwap::new(
        component,                    // ProtocolComponent from Tycho
        swap.token_in.address.clone(),
        swap.token_out.address.clone(),
        0f64,                         // Split: 0 = 100% of amount
        None,                         // No protocol-specific data
        None,                         // No protocol_sim reference
        None,                         // No predetermined output
    );

    Ok(Solution {
        sender: signer_address.clone(),
        receiver: signer_address.clone(),
        given_token: swap.token_in.address.clone(),
        given_amount: swap.amount_in.clone(),
        checked_token: swap.token_out.address.clone(),
        checked_amount: swap.amount_out.clone(),  // Minimum expected output
        exact_out: false,                          // Exact input, not output
        swaps: vec![tycho_swap],
        native_action: None,
    })
}
```

### Encoding the Solution

**File: `crates/core/src/encoder.rs:256-278`**

```rust
pub(crate) fn encode_solution(
    solution: Solution,
    chain: &Chain,
) -> eyre::Result<EncodedSolution> {
    // Set RPC_URL for encoder (required by Tycho)
    if std::env::var("RPC_URL").is_err() {
        unsafe { std::env::set_var("RPC_URL", &chain.rpc_url) };
    }

    // Build the encoder
    let encoder = TychoRouterEncoderBuilder::new()
        .chain(chain.name)  // Tycho Chain enum (Ethereum, Base, Unichain)
        .user_transfer_type(UserTransferType::TransferFromPermit2)
        .build()
        .expect("Failed to build encoder");

    // Encode solution(s)
    let encoded_solutions = encoder
        .encode_solutions(vec![solution.clone()])
        .expect("Failed to encode router calldata");

    Ok(encoded_solutions[0].clone())
}
```

### Building Router Calldata

**File: `crates/core/src/encoder.rs:134-178`**

```rust
pub(crate) fn encode_tycho_router_call(
    chain_id: u64,
    encoded_solution: EncodedSolution,
    solution: &Solution,
    native_address: Bytes,
    signer: PrivateKeySigner,
) -> eyre::Result<TychoTransaction> {
    // Extract and convert Permit2 data
    let permit_data = encoded_solution.permit
        .wrap_err("Permit object must be set")?;
    let permit = PermitSingle::try_from(&permit_data)?;

    // Sign Permit2 approval
    let signature = sign_permit(chain_id, &permit_data, signer)?;

    // Prepare method parameters
    let given_amount = biguint_to_u256(&solution.given_amount);
    let min_amount_out = biguint_to_u256(&solution.checked_amount);
    let given_token = alloyAddress::from_slice(&solution.given_token);
    let checked_token = alloyAddress::from_slice(&solution.checked_token);
    let receiver = alloyAddress::from_slice(&solution.receiver);

    // ABI encode the method call
    let method_calldata = (
        given_amount,
        given_token,
        checked_token,
        min_amount_out,
        false,  // unwrap_native
        false,  // wrap_native
        receiver,
        permit,
        signature.as_bytes().to_vec(),
        encoded_solution.swaps,  // Encoded swap instructions
    ).abi_encode();

    // Prepend function selector
    let calldata = encode_input(
        &encoded_solution.function_signature,
        method_calldata
    );

    // Calculate ETH value (if swapping native token)
    let value = if solution.given_token == native_address {
        solution.given_amount.clone()
    } else {
        BigUint::ZERO
    };

    Ok(TychoTransaction {
        to: encoded_solution.interacting_with,  // TychoRouter address
        value,
        data: calldata,
    })
}
```

### Permit2 Signing

**File: `crates/core/src/encoder.rs:180-198`**

```rust
fn sign_permit(
    chain_id: u64,
    permit_single: &models::PermitSingle,
    signer: PrivateKeySigner,
) -> Result<Signature, EncodingError> {
    // Canonical Permit2 address (same on all chains)
    let permit2_address = alloyAddress::from_str(
        "0x000000000022D473030F116dDEE9F6B43aC78BA3"
    )?;

    // EIP712 domain for Permit2
    let domain = eip712_domain! {
        name: "Permit2",
        chain_id: chain_id,
        verifying_contract: permit2_address,
    };

    let permit = PermitSingle::try_from(permit_single)?;
    let hash = permit.eip712_signing_hash(&domain);

    signer.sign_hash_sync(&hash)
        .map_err(|e| EncodingError::FatalError(format!("Sign failed: {e}")))
}
```

### Complete Encoding Flow

```
Swap (amount_in, amount_out, tokens)
        │
        ▼
create_solution()  ─────────────────►  Solution {
        │                                   sender, receiver,
        │                                   given_token, given_amount,
        │                                   checked_token, checked_amount,
        │                                   swaps: [TychoSwap]
        │                               }
        │
        ▼
encode_solution()  ─────────────────►  EncodedSolution {
        │                                   permit: PermitSingle,
        │                                   function_signature,
        │                                   interacting_with,
        │                                   swaps: encoded bytes
        │                               }
        │
        ▼
encode_tycho_router_call()  ────────►  TychoTransaction {
        │                                   to: TychoRouter address,
        │                                   value: ETH amount,
        │                                   data: ABI-encoded calldata
        │                               }
        │
        ▼
UnsignedTransaction (Alloy TransactionRequest)
```

---

## 4. Protocol Stream & Block Updates

### Overview

The Protocol Stream provides real-time updates of DEX pool states from the Tycho Indexer. It:
- Streams new blocks with pool state changes
- Filters pools by TVL threshold
- Supports multiple DEX protocols (UniV2, V3, V4, Pancake, Sushi)

### Stream Builder Setup

**File: `crates/core/src/collector/tycho.rs:49-101`**

```rust
impl Builder {
    pub async fn build(self) -> eyre::Result<(Handle, JoinHandle<()>)> {
        let protocol_stream = ProtocolStreamBuilder::new(
            &self.tycho_url,
            self.chain,  // tycho_models::Chain enum
        );

        // TVL filter to ignore small pools
        let tvl_filter = ComponentFilter::with_tvl_range(
            self.remove_tvl_threshold,  // e.g., 1.0 (million USD)
            self.add_tvl_threshold,     // e.g., 5.0 (million USD)
        );

        // Register exchanges based on chain
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

        // Configure and build
        let protocol_stream = protocol_stream
            .auth_key(Some(self.tycho_api_key))
            .skip_state_decode_failures(true)
            .set_tokens(token_addresses);  // Filter to specific tokens

        // Build the async stream
        let mut stream = protocol_stream.build().await?;

        // ... spawn worker task
    }
}
```

### Processing Block Updates

**File: `crates/core/src/collector/tycho.rs:165-243`**

```rust
async fn worker(
    mut protocol_stream: impl Stream<Item = Update> + Unpin,
    block_sim_tx: watch::Sender<Arc<Option<Block>>>,
) {
    let mut curr_block_sim: Option<BlockSim> = None;

    while let Some(block_update) = protocol_stream.next().await {
        // Update or initialize BlockSim
        curr_block_sim = Some(match curr_block_sim.take() {
            None => {
                // First block: initialize from scratch
                BlockSim::new(block_update)
            }
            Some(mut block_sim) => {
                // Subsequent blocks: apply incremental update
                block_sim.apply_update(block_update);
                block_sim
            }
        });

        // Broadcast new state to strategy workers
        let _ = block_sim_tx.send(Arc::new(curr_block_sim.clone()));
    }
}
```

### BlockSim State Management

**File: `crates/core/src/state/tycho.rs:36-169`**

```rust
pub struct BlockSim {
    /// Block height or timestamp
    pub block_number_or_timestamp: u64,

    /// Pool states: PoolId -> Arc<dyn ProtocolSim>
    states: HashMap<PoolId, Arc<dyn ProtocolSim>>,

    /// Pool metadata: PoolId -> ProtocolComponent
    metadata: HashMap<PoolId, Arc<ProtocolComponent>>,
}

impl BlockSim {
    /// Create from first Update
    pub fn new(update: Update) -> Self {
        let states = update.states
            .into_iter()
            .map(|(id, state)| (PoolId::from(id), Arc::from(state)))
            .collect();

        let metadata = update.new_pairs
            .into_iter()
            .map(|(id, component)| (PoolId::from(id), Arc::new(component)))
            .collect();

        Self {
            block_number_or_timestamp: update.block_number_or_timestamp,
            states,
            metadata,
        }
    }

    /// Apply incremental update
    pub fn apply_update(&mut self, update: Update) {
        self.block_number_or_timestamp = update.block_number_or_timestamp;

        // Remove pools that no longer meet TVL threshold
        for (id, _) in update.removed_pairs {
            self.states.remove(&PoolId::from(&id));
            self.metadata.remove(&PoolId::from(&id));
        }

        // Add new pools
        for (id, component) in update.new_pairs {
            self.metadata.insert(PoolId::from(&id), Arc::new(component));
        }

        // Update existing pool states
        for (id, state) in update.states {
            self.states.insert(PoolId::from(id), Arc::from(state));
        }
    }
}
```

### Update Structure (from Tycho)

```rust
pub struct Update {
    /// Block number or timestamp
    pub block_number_or_timestamp: u64,

    /// Updated pool states
    pub states: HashMap<String, Box<dyn ProtocolSim>>,

    /// Newly added pools (passed TVL threshold)
    pub new_pairs: HashMap<String, ProtocolComponent>,

    /// Removed pools (dropped below TVL threshold)
    pub removed_pairs: HashMap<String, ProtocolComponent>,
}
```

---

## 5. Transaction Submission

### Overview

Transaction submission uses Alloy (not Tycho) for Ethereum RPC calls:
- Gas estimation: `provider.estimate_gas()`
- Transaction sending: `provider.send_transaction()`
- Receipt waiting: `pending_tx.get_receipt()`

### Gas Estimation

**File: `crates/core/src/encoder.rs:93-107`**

```rust
pub async fn estimate_gas_amount(
    transaction: UnsignedTransaction,
    chain: &Chain,
) -> eyre::Result<u64> {
    let wallet = EthereumWallet::new(chain.signer().clone());
    let provider = alloy::providers::ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(chain.rpc_url.parse()?);

    // TODO: use basefee from signal instead of fetching from RPC
    provider
        .estimate_gas(transaction.tx)
        .await
        .wrap_err("could not estimate gas amount")
}
```

### Transaction Execution

**File: `crates/core/src/encoder.rs:110-132`**

```rust
pub async fn execute_tx(
    transaction: &UnsignedTransaction,
    chain: &Chain,
) -> eyre::Result<TransactionReceipt> {
    let wallet = EthereumWallet::new(chain.signer().clone());
    let provider = alloy::providers::ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(chain.rpc_url.parse()?);

    // Enable Anvil logging for local testing
    provider.anvil_set_logging(true).await.ok();

    // Send transaction
    let pending_tx = provider
        .send_transaction(transaction.tx.clone())
        .await
        .wrap_err("failed sending transaction")?;

    // Wait for receipt
    let receipt = pending_tx
        .get_receipt()
        .await
        .wrap_err("failed getting receipt")?;

    trace!("Transaction mined in block {:?}", receipt.block_number);
    Ok(receipt)
}
```

### Sequential Trade Execution

**File: `crates/core/src/trade.rs:124-208`**

```rust
impl Trade {
    pub async fn run(self, mut id_rx: oneshot::Receiver<i64>) -> eyre::Result<TradeResult> {
        // 1. Estimate gas for both chains
        let slow_gas = estimate_gas_amount(
            self.slow_tx_req.clone(),
            &self.signal.slow_chain
        ).await?;
        let fast_gas = estimate_gas_amount(
            self.fast_tx_req.clone(),
            &self.signal.fast_chain
        ).await?;

        // 2. Check if gas cost exceeds expected profit
        let gas_cost_usdc = (slow_gas + fast_gas) * self.signal.base_fee_usdc;
        if gas_cost_usdc > expected_profit_usdc {
            return Err(eyre!("gas cost exceeds expected profit"));
        }

        // 3. Execute slow chain first
        let slow_receipt = execute_tx(&self.slow_tx_req, &self.signal.slow_chain).await?;
        if !slow_receipt.status() {
            return Ok(TradeResult::FailedSlow(...));
        }

        // 4. Execute fast chain only if slow succeeded
        let fast_receipt = execute_tx(&self.fast_tx_req, &self.signal.fast_chain).await?;
        if !fast_receipt.status() {
            return Ok(TradeResult::FailedFast(...));
        }

        // 5. Calculate realized profit
        let realized_profit = self.calculate_realized_profit(&slow_receipt, &fast_receipt)?;

        Ok(TradeResult::Successful(TradeSuccess {
            slow_receipt,
            fast_receipt,
            realized_profit,
        }))
    }
}
```

---

## 6. Configuration

### Tycho-Related Config Options

**File: `crates/core/src/config.rs`**

```yaml
# kuma.yaml
tycho_api_key: "your-api-key"  # Required for Tycho authentication

chains:
  - name: ethereum
    tycho_url: "tycho-beta.propellerheads.xyz"
    add_tvl_threshold: 5.0      # Min TVL to add pool (millions USD)
    remove_tvl_threshold: 1.0   # TVL to remove pool (millions USD)
```

### Chain to Tycho Chain Mapping

**File: `crates/core/src/chain.rs:38-43`**

```rust
pub fn chain_from_str(s: &str) -> Option<tycho_models::Chain> {
    match s.to_lowercase().as_str() {
        "ethereum" | "mainnet" => Some(tycho_models::Chain::Ethereum),
        "base" => Some(tycho_models::Chain::Base),
        "unichain" => Some(tycho_models::Chain::Unichain),
        _ => None,
    }
}
```

### Environment Variables

```bash
# Tycho API authentication
KUMA_TYCHO_API_KEY=your-api-key

# Or via nested config
KUMA_CHAINS__0__TYCHO_URL=tycho-beta.propellerheads.xyz
```

---

## 7. Data Flow Summary

### Complete Signal Generation Flow

```
1. ProtocolStreamBuilder
   ├── Registers: UniV2, V3, V4, Pancake, Sushi
   ├── Sets TVL filter
   └── Starts WebSocket stream
              │
              ▼
2. BlockUpdate arrives (new block)
   ├── Contains: states, new_pairs, removed_pairs
   └── Processed by BlockSim.apply_update()
              │
              ▼
3. Strategy receives BlockState
   ├── Extract spot prices via pool.spot_price()
   └── Find crossing pools (price discrepancy)
              │
              ▼
4. Precompute swap table (slow chain)
   ├── Call pool.get_amount_out() at various amounts
   └── Store results in Precomputes.pool_sims
              │
              ▼
5. Generate signal (fast chain block arrives)
   ├── Binary search over precomputed swaps
   ├── Simulate fast chain via pool.get_amount_out()
   └── Calculate expected profit in USDC
              │
              ▼
6. Encode transaction
   ├── create_solution() → Solution
   ├── encode_solution() → EncodedSolution
   └── encode_tycho_router_call() → calldata
              │
              ▼
7. Execute trade
   ├── estimate_gas() → verify profitability
   ├── execute_tx() slow chain → wait for receipt
   └── execute_tx() fast chain → wait for receipt
              │
              ▼
8. Calculate realized profit
   └── Parse swap logs from receipts
```

### Key Tycho Methods Used

| Method | Purpose | Location |
|--------|---------|----------|
| `ProtocolStreamBuilder::new()` | Create stream builder | collector/tycho.rs |
| `.exchange::<State>()` | Register DEX protocol | collector/tycho.rs |
| `.build().await` | Start WebSocket stream | collector/tycho.rs |
| `stream.next()` | Get next BlockUpdate | collector/tycho.rs |
| `pool.get_amount_out()` | Simulate swap | strategy/simulation.rs |
| `pool.spot_price()` | Get pool price | spot_prices.rs |
| `TychoRouterEncoderBuilder::new()` | Create encoder | encoder.rs |
| `encoder.encode_solutions()` | Encode for router | encoder.rs |

---

*Last Updated: 2026-01-17*
