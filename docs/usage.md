# Using Kuma

Learn the essential commands and features to use Kuma effectively.

## Essential Commands

### Find Trading Opportunities
```bash
# Find arbitrage opportunities between Ethereum and Unichain
just generate-signals token-a="UNI" token-b="WETH" slow-chain="ethereum" fast-chain="unichain"

# Try different token pairs
just generate-signals token-a="USDC" token-b="WETH" slow-chain="ethereum" fast-chain="unichain"
```

### Test Trades Safely
```bash
# Simulate a trade without spending real money (always do this first!)
cargo run -p kuma-cli dry-run --input-path ./fake_signal.json --output-path ./result.json

# Check what would happen
cat ./result.json
```

### Check Token Balances
```bash
# See available tokens and balances on Ethereum
just get-tokens chain="ethereum"

# Check Unichain tokens
just get-tokens chain="unichain"
```

## Web Interface

Open http://localhost:3000 to access the dashboard.

### Main Dashboard
- **Live Prices**: Current token prices across chains
- **Recent Signals**: Latest arbitrage opportunities found
- **Performance**: Profit/loss tracking

### Key Pages
- **Spot Prices**: Historical price charts and data
- **Signals**: Detailed arbitrage opportunity list  
- **System Status**: Health checks and service status

## Basic Workflow

### 1. Start Kuma
```bash
just docker-run  # Start all services
```

### 2. Monitor Prices
- Open web interface at http://localhost:3000
- Watch for price differences between chains
- Review generated signals

### 3. Test Before Trading
```bash
# Always test first with dry-run
cargo run -p kuma-cli dry-run --input-path ./signal.json --output-path ./test.json
cat ./test.json  # Check profitability
```

### 4. Execute Trades (Optional)
```bash
# Only after testing! This uses real money
cargo run -p kuma-cli execute --input-path ./validated_signal.json
```

## Service Management

```bash
# Start/stop all services
just docker-run    # Start everything
just docker-stop   # Stop everything

# Individual services
just backend       # API server only
just webapp-dev    # Web interface only
just kumad-start   # Trading daemon only

# Database management
just db-start      # Start database
just db-reset      # Reset database (deletes all data)
```

## Monitoring

### Check System Health
```bash
# API health check
curl http://localhost:8080/health

# Service status
docker ps

# View logs
docker logs kumad     # Trading daemon logs
docker logs kuma-api  # API server logs
```

### Data Access
```bash
# Recent price data
curl "http://localhost:8080/spot_prices?page=1&page_size=10"

# Recent signals
curl "http://localhost:8080/signals?page=1&page_size=10"
```

## Safety Tips

- **Always dry-run first**: Never execute trades without testing
- **Start small**: Use minimal amounts when testing with real funds  
- **Monitor closely**: Watch for network congestion and gas price spikes
- **Have exit plans**: Know how to stop automated trading if needed
- **Test configuration**: Verify all settings before live trading

## Common Commands Quick Reference

```bash
# Essential workflow
just docker-run                                    # Start everything
just generate-signals token-a="UNI" token-b="WETH" # Find opportunities  
cargo run -p kuma-cli dry-run --input-path ...     # Test safely
just get-tokens chain="ethereum"                   # Check balances

# Debugging  
docker logs kumad                                   # Check daemon logs
curl http://localhost:8080/health                  # Check API health
just db-reset                                      # Reset if issues
```