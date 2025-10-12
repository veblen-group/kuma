# Kuma Documentation

Kuma is a cross-chain arbitrage trading bot that automatically finds profitable trading opportunities between different blockchain networks.

## What is Kuma?

Kuma monitors cryptocurrency prices across Ethereum and Unichain networks, finds price differences for the same tokens, and can execute trades to profit from these differences. Think of it as an automated trader that works 24/7 to find arbitrage opportunities.

## Quick Start

**Want to try it right now?**

```bash
git clone https://github.com/your-org/kuma.git
cd kuma
just docker-run
```

Then open http://localhost:3000 to see the web interface.

## Documentation

### Essential Guides
- [**Getting Started**](getting-started.md) - Install and run Kuma in 5 minutes
- [**Configuration**](configuration.md) - Set up trading pairs and chains  
- [**Usage Guide**](usage.md) - Basic commands and web interface

### Technical Documentation
- [**Overview**](overview.md) - How Kuma works under the hood
- [**API Reference**](api-reference.md) - REST API and integration
- [**Architecture**](architecture.md) - System design details
- [**Deployment**](deployment.md) - Production deployment guide

## Key Features

- 🔄 **Cross-Chain Trading**: Arbitrage between Ethereum and Unichain
- 📊 **Real-Time Monitoring**: Live price tracking and opportunity detection  
- 🌐 **Web Dashboard**: Monitor performance and view trading signals
- 🛡️ **Risk Management**: Built-in slippage protection and safety checks
- 🐳 **Dry Run Mode**: Test strategies without risking real money

## Need Help?

- **Problems?** Check the logs: `docker logs kumad` or `docker logs kuma-api`
- **Common Issues**: 
  - Port conflicts: Kill processes using `pkill -f kuma` then restart
  - Database issues: Run `just db-reset` to reset everything
  - Build errors: Run `docker system prune` then rebuild
- **Questions?** Submit issues on the project repository