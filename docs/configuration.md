# Configuration

Learn how to configure Kuma for your trading needs.

## Quick Start

The default `kuma.yaml` works for testing. Here are the key settings you might want to change:

```yaml
# Which token pairs to trade
strategies:
  - token_a: UNI          # First token
    token_b: WETH         # Second token  
    slow_chain: ethereum  # More expensive chain
    fast_chain: unichain  # Cheaper chain

# Safety settings
max_slippage_bps: 25      # Max 0.25% slippage allowed
```

## Main Settings

### Adding Trading Pairs
```yaml
strategies:
  # Trade UNI for WETH
  - token_a: UNI
    token_b: WETH
    slow_chain: ethereum
    fast_chain: unichain
    
  # Trade USDC for WETH  
  - token_a: USDC
    token_b: WETH
    slow_chain: ethereum
    fast_chain: unichain
```

### Safety Settings
```yaml
# Risk management
max_slippage_bps: 25        # Max 0.25% slippage (25 basis points)
congestion_risk_discount_bps: 0  # Network congestion discount

# Liquidity requirements
add_tvl_threshold: 5.0      # Minimum $5M liquidity to consider pools
remove_tvl_threshold: 1.0   # Remove pools below $1M liquidity
```

### Server Settings
```yaml
server:
  host: "0.0.0.0"  # Listen on all interfaces
  port: 8080       # API server port

database:
  host: "localhost"  # Database location
  port: 5432        # Database port
  # Other database settings use defaults
```

## Environment Variables (For Production)

Override settings with environment variables:

```bash
# Database settings
export KUMA_DATABASE_HOST="your-db-host"
export KUMA_DATABASE_PASSWORD="secure_password"

# Private keys (keep these secret!)
export KUMA_CHAINS__0__PRIVATE_KEY="0xYOUR_ETHEREUM_PRIVATE_KEY"
export KUMA_CHAINS__1__PRIVATE_KEY="0xYOUR_UNICHAIN_PRIVATE_KEY"

# API keys
export KUMA_TYCHO_API_KEY="your_tycho_api_key"
```

## Complete Configuration Example

Here's what a typical `kuma.yaml` looks like:

```yaml
# Database (usually don't need to change for Docker setup)
database:
  host: "localhost"
  port: 5432
  user: "api_user"
  password: "password"

# API server
server:
  host: "0.0.0.0"
  port: 8080

# Trading pairs to monitor
strategies:
  - token_a: UNI
    token_b: WETH
    slow_chain: ethereum
    fast_chain: unichain
  - token_a: USDC  
    token_b: WETH
    slow_chain: ethereum
    fast_chain: unichain

# Safety settings
max_slippage_bps: 25              # 0.25% max slippage
add_tvl_threshold: 5.0            # $5M minimum liquidity
tycho_api_key: "your_api_key"     # Get from Tycho

# Blockchain connections
chains:
  - name: ethereum
    rpc_url: "https://ethereum-rpc.publicnode.com"
    tycho_url: "tycho-beta.propellerheads.xyz"
    # private_key set via environment variable for security

  - name: unichain  
    rpc_url: "https://mainnet.unichain.org"
    tycho_url: "tycho-unichain-beta.propellerheads.xyz"
    # private_key set via environment variable for security
```

⚠️ **Note**: Token addresses and other advanced settings are pre-configured in the default `kuma.yaml`. You typically don't need to modify these unless adding new tokens or chains.

## Key Settings Explained

| Setting | What it does | Example |
|---------|--------------|---------|
| `max_slippage_bps` | Maximum price movement allowed during trade | `25` = 0.25% |
| `add_tvl_threshold` | Minimum liquidity required for trading pools | `5.0` = $5 million |
| `strategies` | Which token pairs to monitor for arbitrage | UNI/WETH, USDC/WETH |

## Security Best Practices

1. **Never commit private keys** to version control
2. **Use environment variables** for sensitive data in production
3. **Start with test networks** before using mainnet
4. **Use minimal amounts** when testing with real funds