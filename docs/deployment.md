# Deployment Guide

This guide covers production deployment of Kuma using Docker, cloud platforms, and best practices for security and monitoring.

## Table of Contents

- [Docker Deployment](#docker-deployment)
- [Production Configuration](#production-configuration)

## Docker Deployment

### Quick Production Deployment

```bash
# Clone and build
git clone https://github.com/your-org/kuma.git
cd kuma

# Build production images
just docker-build-all version="latest"

# Start all services
docker-compose --profile all up -d

# Initialize database
just kumad-init

# Verify deployment
curl http://localhost:8080/health
curl http://localhost:3000
```

### Docker Compose Profiles

Kuma uses Docker Compose profiles for flexible deployment:

```bash
# Database only
docker-compose --profile db up -d

# Daemon with database
docker-compose --profile kumad up -d

# Web application with API and database  
docker-compose --profile webapp up -d

# Everything
docker-compose --profile all up -d
```

### Application Configuration

Create `kuma.prod.yaml`:

```yaml
# Production configuration
database:
  host: "postgres"  # Docker service name
  port: 5432
  dbname: "kuma_prod"
  user: "kuma_prod_user"
  # password set via environment variable
  max_connections: 20
  connection_timeout_secs: 60
  idle_timeout_secs: 900

server:
  host: "0.0.0.0"
  port: 8080

# Production strategies
strategies:
  - token_a: UNI
    token_b: WETH
    slow_chain: ethereum
    fast_chain: unichain
  - token_a: USDC
    token_b: WETH
    slow_chain: ethereum
    fast_chain: unichain

# Stricter production parameters
binary_search_steps: 32768
congestion_risk_discount_bps: 50
max_slippage_bps: 15  # 0.15% max slippage
add_tvl_threshold: 10.0  # Higher liquidity requirements
remove_tvl_threshold: 5.0

# Production chains with environment-based private keys
chains:
  - name: ethereum
    rpc_url: "https://ethereum-rpc.production.com"
    tycho_url: "tycho-beta.propellerheads.xyz"
    permit2_address: "0x000000000022d473030f116ddee9f6b43ac78ba3"
    # private_key set via KUMA_CHAINS__0__PRIVATE_KEY

  - name: unichain
    rpc_url: "https://mainnet.unichain.org"
    tycho_url: "tycho-unichain-beta.propellerheads.xyz"
    permit2_address: "0x000000000022d473030f116ddee9f6b43ac78ba3"
    # private_key set via KUMA_CHAINS__1__PRIVATE_KEY

# Production tokens (same as development but verified addresses)
tokens:
  UNI:
    addresses:
      ethereum: "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984"
      unichain: "0x8f187aa05619a017077f5308904739877ce9ea21"
    decimals: 18
    tax: 1000
    gas: [1000]
    quality: 100
    inventory: 100  # Conservative inventory for production
    
  WETH:
    addresses:
      ethereum: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
      unichain: "0x4200000000000000000000000000000000000006"
    decimals: 18
    tax: 1000
    gas: [1000]
    quality: 100
    inventory: 10
    
  USDC:
    addresses:
      ethereum: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
      unichain: "0x078D782b760474a361dDA0AF3839290b0EF57AD6"
    decimals: 6
    tax: 1000
    gas: [1000]
    quality: 100
    inventory: 50000
```
