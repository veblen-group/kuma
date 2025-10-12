# Getting Started

Get Kuma up and running in 5 minutes.

## Quick Start (Recommended)

The fastest way to try Kuma:

```bash
# Clone the project
git clone https://github.com/your-org/kuma.git
cd kuma

# Start everything with Docker
just docker-run

# Wait a minute for services to start, then check:
open http://localhost:3000  # Web interface
curl http://localhost:8080/health  # API health check
```

That's it! Kuma is now running with:
- Web dashboard at http://localhost:3000
- API server at http://localhost:8080  
- Database and all services

## What You Need

### For Quick Start (Docker)
- **Docker Desktop**: Download from [docker.com](https://www.docker.com/get-started)
- **Git**: Usually pre-installed, or get from [git-scm.com](https://git-scm.com)

### For Development
If you want to modify the code:
- **Rust**: Install from [rustup.rs](https://rustup.rs)
- **Just**: Run `cargo install just` (command runner tool)

## Installation Options

### Option 1: Docker (Easiest)
```bash
git clone https://github.com/your-org/kuma.git
cd kuma
just docker-run
```

### Option 2: Development Setup
```bash
# Install prerequisites
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo install just

# Clone and build
git clone https://github.com/your-org/kuma.git
cd kuma
cargo build

# Start database
just db-start
just db-migrate

# Start services
just backend &    # API server
just webapp-dev   # Web interface
```

## First Steps

Once Kuma is running, try these commands:

### 1. Check Everything is Working
```bash
# Check the web interface
open http://localhost:3000

# Check API health
curl http://localhost:8080/health

# View recent price data
curl "http://localhost:8080/spot_prices?page=1&page_size=5"
```

### 2. Generate Your First Trading Signal
```bash
# This finds arbitrage opportunities for UNI/WETH between Ethereum and Unichain
just generate-signals token-a="UNI" token-b="WETH" slow-chain="ethereum" fast-chain="unichain"
```

### 3. Try a Dry Run (Simulation)
```bash
# Simulate a trade without executing it (safe)
cargo run -p kuma-cli dry-run --input-path ./fake_signal.json --output-path ./result.json

# Check the results  
cat ./result.json
```

## Basic Configuration

The default configuration works out of the box for testing. The main settings are in `kuma.yaml`:

```yaml
# Which token pairs to monitor
strategies:
  - token_a: UNI
    token_b: WETH
    slow_chain: ethereum  # More expensive chain
    fast_chain: unichain  # Cheaper/faster chain

# Risk settings
max_slippage_bps: 25     # Maximum 0.25% slippage
```

⚠️ **Important**: The default configuration includes test keys. Never use real money with the default setup!

## Common Issues

### Port Already in Use
```bash
# If you get "port already in use" errors
pkill -f kuma
just docker-run
```

### Database Issues
```bash
# If database won't start or has errors
just db-reset  # This deletes all data and starts fresh
```

### Services Won't Start
```bash
# Check what's running
docker ps

# Check logs if something is failing
docker logs kumad
docker logs kuma-api
docker logs kuma-db
```

### Build Errors
```bash
# Clean up and rebuild
docker system prune -f
just docker-build-all
just docker-run
```

## Next Steps

Once you have Kuma running:
1. **Read the [Configuration Guide](configuration.md)** to customize trading pairs
2. **Check the [Usage Guide](usage.md)** to learn essential commands  
3. **Review the [Overview](overview.md)** to understand how it works