# How Kuma Works

## The Simple Explanation

Kuma is like having a trading assistant that watches prices on different blockchain networks and automatically finds profitable opportunities.

Here's what it does:
1. **Watches Prices**: Monitors token prices on selected chains
2. **Finds Differences**: When the same token has different prices on different chains
3. **Calculates Profit**: Determines if the price difference is profitable after fees
4. **Can Execute Trades**: Optionally execute the arbitrage automatically

## Example Scenario

```
Ethereum: 1 WETH = 1,800 USDC
Unichain: 1 WETH = 1,795 USDC

Opportunity: Buy WETH cheap on Unichain, sell expensive on Ethereum
Potential Profit: ~5 USDC per WETH (minus fees and gas)
```

## Main Components

### 🤖 **Daemon Service (kumad)**
The "brain" that runs continuously:
- Monitors prices 24/7
- Detects arbitrage opportunities
- Can execute trades automatically

### 🌐 **Web Dashboard**
Simple interface to:
- View current prices
- See detected opportunities
- Monitor trading performance
- Check system status

### ⚙️ **CLI Tool**
Command-line interface for:
- Manual testing and operations
- Generating signals on demand
- Dry-run simulations
- System administration

### 📊 **API Server**
Provides data access for:
- Historical price data
- Trading signal history
- System integration

## Safety Features

- **Dry Run Mode**: Test strategies without spending real money
- **Slippage Protection**: Abort trades if prices move too much
- **Risk Assessment**: Calculate potential losses before trading
- **Manual Override**: Always have control over automated trading

## How It Works

### 1. Price Collection
Kuma continuously monitors spot prices across supported blockchain networks using the Tycho protocol. It tracks:
- Token pair prices on various DEXs
- Block heights for timing accuracy
- Pool liquidity and trading volumes

### 2. Arbitrage Detection
The system analyzes collected price data to identify opportunities where:
- Price differences exceed minimum profit thresholds
- Sufficient liquidity exists for profitable trades
- Network conditions allow for timely execution

### 3. Strategy Execution
When profitable opportunities are detected, Kuma can:
- Generate detailed trading signals with expected profits
- Execute trades automatically through the daemon service
- Provide simulation results through dry-run mode

### 4. Data Storage & Analysis
All market data and trading signals are stored in a PostgreSQL database for:
- Historical analysis and backtesting
- Performance monitoring and optimization
- Compliance and audit requirements

## Core Components

### Kuma Core (`kuma-core`)
The foundational library containing:
- Configuration management
- Database models and operations
- Trading strategy implementations
- Blockchain interaction utilities
- Risk management algorithms

### CLI Tool (`kuma-cli`)
Command-line interface providing:
- Manual signal generation
- Token information retrieval
- Permit2 initialization
- Dry-run simulations
- Strategy testing capabilities

### Daemon Service (`kumad`)
Long-running background service that:
- Monitors markets continuously
- Executes trading strategies automatically
- Manages connection pooling and error recovery
- Provides telemetry and logging

### Backend API (`kuma-backend`)
RESTful API server offering:
- Spot price data access
- Trading signal history
- Real-time market data
- System status and health checks

### Web Interface (`webapp`)
React/Next.js dashboard featuring:
- Real-time market data visualization
- Trading signal monitoring
- Performance analytics
- System configuration interface

## Supported Assets

### Primary Trading Pairs
- **UNI/WETH**: Uniswap governance token paired with wrapped Ethereum
- **USDC/WETH**: Stablecoin arbitrage opportunities
- **WBTC/WETH**: Bitcoin-Ethereum cross-asset arbitrage

### Token Standards
- **ERC-20**: Standard fungible token support
- **Permit2**: Gasless approval mechanisms
- **Multi-decimal**: Support for tokens with different decimal places

## Use Cases

### 1. Automated Trading
- Deploy the daemon service for continuous market monitoring
- Execute arbitrage opportunities automatically
- Maximize returns through 24/7 operation

### 2. Strategy Development
- Use CLI tools for backtesting and strategy refinement
- Dry-run mode for risk-free strategy validation
- Custom strategy development and testing

### 3. Market Analysis
- Historical data analysis through API endpoints
- Performance tracking and optimization
- Market research and opportunity identification

### 4. Portfolio Management
- Integration with existing trading infrastructure
- Risk-adjusted return optimization
- Compliance and reporting capabilities

## Getting Started

1. **[Installation](getting-started.md)**: Set up the development environment
2. **[Configuration](configuration.md)**: Configure blockchain networks and trading parameters
3. **[Usage](usage.md)**: Learn CLI commands and basic operations
4. **[Deployment](deployment.md)**: Deploy for production use

## Security Considerations

### Private Key Management
- Store private keys securely using environment variables
- Use hardware wallets or key management services for production
- Never commit private keys to version control

### Network Security
- Use HTTPS endpoints for all external API calls
- Implement rate limiting for API endpoints
- Monitor for unusual trading patterns or system access

### Risk Management
- Start with small position sizes for testing
- Implement stop-loss mechanisms where appropriate
- Monitor system performance and market conditions continuously

## License

Kuma is released under the MIT License. See [LICENSE](../LICENSE) for details.