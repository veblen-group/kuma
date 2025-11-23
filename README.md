# kuma
![kuma](kuma.png)

Cross-chain arbitrage bot for [Tycho Community Extensions TAP-6](https://github.com/propeller-heads/tycho-x/blob/main/TAP-6.md).

[Proposal link](https://hackmd.io/qU6uAJ2BQe2pDwI2k9m_XA) (or in the [docs](docs/tap6-proposal.md) directory).

## Local Development
See [Prerequisites](#prerequisites) to make sure all dependencies are installed.

Follow these steps to get the full system running locally:

1. **Start the database**:
   ```bash
   just db-start
   ```

2. **Start the backend API server**:
   ```bash
   just backend
   ```

3. **Build and start the webapp**:
   ```bash
   just webapp-build
   just webapp-start
   ```
   The webapp will be available at `http://localhost:3000`.

4. **Run kumad (the arbitrage daemon)**:
   ```bash
   just kumad
   ```

   For split-screen logging (see [Running kumad with split-screen logs](#running-kumad-with-split-logs)):
   ```bash
   just kumad-split
   ```

**Note**: Signing Permit2 is required only once per chain per wallet. Use the CLI utility to sign Permit2 payloads (see [CLI Utilities](#cli-utilities)).

## CLI Utilities

The `kuma-cli` provides several utilities for testing and interacting with the system:

### Generate Signals
```bash
just generate-signal usdc weth ethereum unichain
```

This command will try to generate a signal from the next block on the provided chains.

### Dry Run
```bash
just dry-run signal.json trade.json
```

Encodes a signal into calldata and signs the transactions, saving the unexecuted trade to the provided output path.

### Trade Execution
```bash
just execute-trade trade.json
```

Executes a trade from the signed transaction data created by `dry-run`.

### Get Token Data
```bash
just get-tokens ethereum # alt: base, unichain
```

This retrieves comprehensive token data including addresses, decimals, tax information, and gas costs. The output can be used to populate the `tokens` section of `kuma.yaml`.

### Sign Permit2
```bash
just init-permit2
```

This signs the Permit2 approval that allows the arbitrage contract to spend your tokens. **This only needs to be done once per chain per wallet, ever.** The approval persists across all future trades.

The permit will be signed for all the tokens configured for the given chain in [`kuma.yaml`](kuma.yaml)

After signing, you can execute trades without needing additional approvals.


### Configuration (`kuma.yaml`)

The `kuma.yaml` file contains all configuration for the arbitrage bot. Here's what each section does:

#### Strategy Configuration

Define trading pairs to monitor for arbitrage opportunities:

```yaml
strategies:
  - token_a: UNI
    token_b: WETH
    slow_chain: ethereum
    fast_chain: unichain
```
To add a new strategy, simply add another entry to the `strategies` list with your desired token pair and chains.
- **token_a / token_b**: The token pair to trade. Must match token symbols defined in the `tokens` section
- **slow_chain / fast_chain**: The chains to arbitrage between. The "slow chain" is where you buy, the "fast chain" is where you sell

**Strategy parameters:**
```
binary_search_steps: 16384
congestion_risk_discount_bps: 10
max_slippage_bps: 25
```
- **binary_search_steps**: Number of steps for binary search optimization when finding optimal trade amounts
- **congestion_risk_discount_bps**: Discount applied to profit calculations to account for settlement risk due to  network congestion (in basis points, 10 = 0.1%)
- **max_slippage_bps**: Maximum acceptable slippage for trades (in basis points, 25 = 0.25%)

**Tycho TVL threshold:**
```yaml
add_tvl_threshold: 5.0
remove_tvl_threshold: 1.0
```

Controls which pools are monitored based on Total Value Locked (TVL), in units of $1MM:
- **add_tvl_threshold**: Minimum TVL to start monitoring a pool
- **remove_tvl_threshold**: TVL below which a pool is removed from monitoring

#### Token Configuration
Define tokens with their addresses, decimals, and other properties:

```yaml
tokens:
  UNI:
    addresses:
      ethereum: "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984"
      base: "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984"
      unichain: "0x8f187aa05619a017077f5308904739877ce9ea21"
    decimals: 18
    tax: 1000
    gas:
      - 1000
    quality: 100
    inventory:
      ethereum: 500
      base: 500
      unichain: 500
```

- **addresses**: Token contract address on each chain
- **decimals**: Token decimals (e.g., 18 for most ERC20 tokens, 6 for USDC)
- **tax**: Transfer tax in basis points (1000 = 10%, 0 = no tax)
- **gas**: Gas costs for token transfers
- **quality**: Token quality score (0-100)
- **inventory**: Available balance on each chain for arbitrage

**Getting token data**: Use the committed `tokens.<chain>.json` files in the repository root as reference, or fetch fresh data using:

```bash
just get-tokens ethereum # alt: base, unichain, etc.
```

**Generating token JSON files**: The CLI can generate token configuration from on-chain data. See [CLI Utilities](#cli-utilities) for details.

#### Chain Configuration
Define RPC endpoints and chain-specific settings:

```yaml
chains:
  - name: ethereum
    rpc_url: "https://eth-mainnet.g.alchemy.com/v2/"
    rpc_ws_url: "wss://eth-mainnet.g.alchemy.com/v2/"
    tycho_url: "tycho-beta.propellerheads.xyz/"
    permit2_address: "0x000000000022d473030f116ddee9f6b43ac78ba3"
    private_key: ""

tycho_api_key: ""
```

- **rpc_url / rpc_ws_url**: HTTP and WebSocket RPC endpoints
- **tycho_url**: Tycho simulation API endpoint for the chain
- **permit2_address**: Permit2 contract address (typically the same across chains)
- **private_key**: Private key for the wallet executing trades
- **tycho_api_key**: API key for Tycho services.

#### Database and Backend Configuration
For local development:

```yaml
database:
  user: "api_user"
  password: "password"
  host: "localhost"
  port: 5432
  dbname: "api_db"
  max_connections: 10
  connection_timeout_secs: 30
  idle_timeout_secs: 600

server:
  host: "0.0.0.0"
  port: 8080
```

These settings configure the PostgreSQL database connection and the backend API server that the webapp connects to.

### Running kumad with Split Logs
The `just kumad-split` command runs kumad with logs split into two tmux panes for easier debugging:
- **Top pane**: Info, warn, and error logs
- **Bottom pane**: Debug and trace logs (verbose output)

**Prerequisites**:
- `tmux` must be installed
- `less` must be available (for paging through logs)
- The `run_split.sh` script must be executable: `chmod +x run_split.sh`

Usage:
```bash
just kumad-split
```

This is especially useful when debugging as it separates high-level operational logs from verbose trace information.

### Database Management
#### Database Schema
Run the migrations to set up the required tables:

```sql
-- See migrations/001_initial.sql for the complete schema
```
The schema includes tables for:
- `spot_prices`: Token pair spot price data indexed by pool and block height
- `signals`: Cross-chain arbitrage opportunities with full swap details
- `trades`: TODO

#### Seed data for `webapp` testing
TODO

#### Reset the database
```bash
just db-reset
```

TODO

#### Compile-Time Query Validation
When compiling the backend, SQLx will validate all queries at compile time. This ensures that any SQL errors are caught early and prevents runtime errors.

Queries that have been modified need to be recompiled with SQLx CLI so they can be checked without requiring a DB connection in build time (["offline mode"](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md#enable-building-in-offline-mode-with-query)):

```bash
cargo sqlx prepare --database-url $DATABASE_URL

# Or more simply, with the just command from the workspace root:
just db-prepare
```

If the database schema is modified, you may need to reset the database and run migrations again before recompiling with the SQLx CLI.

## Prerequisites
- [Docker](https://docs.docker.com/get-docker/) and Docker Compose
- [Rust](https://rustup.rs/) (latest stable)
- [SQLx CLI](https://github.com/launchbadge/sqlx/tree/main/sqlx-cli)
- [Cargo SQLx Build Tool](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md#with-rust-toolchain)
- [tmux](https://github.com/tmux/tmux/wiki) (optional, for `just kumad-split`)
