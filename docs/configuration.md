---
title: Configuration
description: kuma.yaml schema and environment variable reference.
updated: 2026-04-13
---

# Configuration

`crates/core/src/config.rs` — loaded via `Config::load()`

Configuration is read from `kuma.yaml` and can be overridden by environment variables prefixed with `KUMA_` and using `__` as a separator for nested keys (powered by [Figment](https://docs.rs/figment)).

## Top-level keys

```yaml
tycho_api_key: "your-key"       # Tycho Indexer authentication

binary_search_steps: 20         # Number of inventory steps precomputed on the slow chain.
                                 # Higher = finer-grained optimal amount search, slower precompute.

max_slippage_bps: 50            # Maximum acceptable slippage in basis points (50 = 0.5%).
                                 # Applied to amount_out when constructing the fast leg's amount_in.

congestion_risk_discount_bps: 200  # Flat profit discount for pool congestion risk (200 = 2%).

# Disable specific discount components (still calculated and logged, just not gating trades)
ignore_gas_costs_in_profit: false
ignore_slippage_in_profit: false
ignore_congestion_fee_in_profit: false
ignore_usdc_conversion_in_profit: false
```

## Strategies

```yaml
strategies:
  - token_a: USDC
    token_b: WETH
    slow_chain: ethereum   # must match a name in `chains`
    fast_chain: unichain
```

Multiple strategies can run in parallel. Each pair `(token_a, token_b, slow_chain, fast_chain)` spawns its own collector set, strategy worker, and signal channel.

## Tokens

```yaml
tokens:
  USDC:
    decimals: 6
    tax: 0
    gas: [null]
    quality: 100
    addresses:
      ethereum: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
      base:     "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913"
      unichain: "0x078d782b760474a361dda0af3839290b0ef57ad6"
    inventory:
      ethereum: "1000000000"   # raw units (USDC has 6 decimals → $1000)
      base:     "1000000000"
      unichain: "1000000000"
```

`inventory` is the maximum capital available per token per chain for the binary search. It caps the trade size and affects precompute range.

## Chains

```yaml
chains:
  - name: ethereum
    rpc_url: "https://mainnet.infura.io/v3/..."
    rpc_ws_url: "wss://mainnet.infura.io/ws/v3/..."
    tycho_url: "tycho-beta.propellerheads.xyz"
    permit2_address: "0x000000000022D473030F116dDEE9F6B43aC78BA3"
    private_key: "0x..."
    add_tvl_threshold: 5.0    # add pool when TVL ≥ $5M
    remove_tvl_threshold: 1.0 # remove pool when TVL < $1M
```

## Database & server

```yaml
database:
  user: api_user
  password: password
  host: localhost
  port: 5432
  dbname: api_db
  max_connections: 10
  connection_timeout_secs: 30
  idle_timeout_secs: 600

server:
  host: "0.0.0.0"
  port: 8080
```

## Environment variable overrides

```bash
KUMA_TYCHO_API_KEY=xxx
KUMA_CHAINS__0__PRIVATE_KEY=0x...
KUMA_BINARY_SEARCH_STEPS=30
KUMA_MAX_SLIPPAGE_BPS=100
```

Copy `kuma.yaml` to `kuma.prod.yaml` for production and use `just push-prod-config` to deploy it. The `.gitignore` excludes `kuma.prod.yaml` to prevent secret leakage.
