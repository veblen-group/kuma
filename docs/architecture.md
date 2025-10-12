# Kuma Architecture

This document provides a comprehensive overview of Kuma's system architecture, component design, and data flow.

## System Overview

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Web Frontend  │    │  Backend API    │    │     kumad       │
│   (Next.js)     │◄──►│  (Rust/Axum)   │◄──►│   (Daemon)      │
│     :3000       │    │     :8080       │    │                 │
└─────────────────┘    └─────────────────┘    └─────────────────┘
                                │                       │
                                ▼                       ▼
                       ┌─────────────────┐    ┌─────────────────┐
                       │   PostgreSQL    │    │  Blockchain     │
                       │    Database     │    │    Networks     │
                       │     :5432       │    │ (Ethereum/Uni)  │
                       └─────────────────┘    └─────────────────┘
                                                        │
                                                        ▼
                                                ┌─────────────────┐
                                                │ Tycho Protocol  │
                                                │   Simulation    │
                                                └─────────────────┘
```

## Core Components

### 1. Kuma Core (`kuma-core`)

The foundational library that provides shared functionality across all components.

**Location**: `crates/core/`

**Key Modules**:
- `config.rs`: Configuration management using Figment
- `chain.rs`: Blockchain network abstractions
- `signals.rs`: Arbitrage signal generation and processing
- `spot_prices.rs`: Price data collection and analysis
- `database/`: Database models and operations
- `strategy/`: Trading strategy implementations
- `collector/`: Data collection from blockchain networks
- `state/`: State management for blocks, pairs, and market data

**Responsibilities**:
- Configuration loading and validation
- Database connection management
- Blockchain interaction utilities
- Core business logic for arbitrage detection
- Data models and serialization

### 2. CLI Tool (`kuma-cli`)

Command-line interface for manual operations and testing.

**Location**: `crates/cli/`

**Key Files**:
- `main.rs`: Entry point and signal handling
- `cli.rs`: Clap-based command definitions
- `dryrun.rs`: Simulation without execution
- `execute.rs`: Trade execution logic
- `tokens.rs`: Token information retrieval
- `permit.rs`: Permit2 initialization

**Commands**:
```bash
generate-signals    # Generate arbitrage signals
dry-run            # Simulate trades
execute            # Execute actual trades  
tokens             # List available tokens
init-permit2       # Initialize Permit2 approvals
```

### 3. Daemon Service (`kumad`)

Long-running background service for automated trading.

**Location**: `crates/kumad/`

**Key Components**:
- `main.rs`: Service initialization and shutdown handling
- `lib.rs`: Core daemon logic
- `telemetry.rs`: Logging and metrics collection
- `strategy/`: Strategy execution engine

**Features**:
- Continuous market monitoring
- Automated signal generation and execution
- Connection pooling and error recovery
- Graceful shutdown handling
- Telemetry and observability

### 4. Backend API (`kuma-backend`)

RESTful API server for data access and system monitoring.

**Location**: `crates/backend/`

**Features**:
- Spot price data endpoints
- Trading signal history
- Real-time market data
- System health checks
- PostgreSQL integration with SQLx

**API Endpoints**:
- `GET /spot_prices` - Historical price data
- `GET /signals` - Arbitrage signals
- Health checks and metrics endpoints

### 5. Web Interface (`webapp`)

React/Next.js dashboard for monitoring and visualization.

**Location**: `webapp/`

**Features**:
- Real-time data visualization
- Trading signal monitoring
- Performance analytics
- Responsive design with Tailwind CSS
- TypeScript for type safety

## Data Flow Architecture

### 1. Data Collection Pipeline

```
Blockchain Networks
        │
        ▼
┌─────────────────┐
│ Tycho Protocol  │
│   API Clients   │
└─────────────────┘
        │
        ▼
┌─────────────────┐
│  Price Collector │
│   (kuma-core)   │
└─────────────────┘
        │
        ▼
┌─────────────────┐
│   PostgreSQL    │
│   spot_prices   │
└─────────────────┘
```

### 2. Signal Generation Pipeline

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Spot Prices   │───►│ Strategy Engine │───►│     Signals     │
│   (Database)    │    │  (kuma-core)    │    │   (Database)    │
└─────────────────┘    └─────────────────┘    └─────────────────┘
                                │
                                ▼
                       ┌─────────────────┐
                       │ Risk Assessment │
                       │  & Validation   │
                       └─────────────────┘
```

### 3. Execution Pipeline

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│     Signals     │───►│  Execution      │───►│   Blockchain    │
│   (Database)    │    │   Engine        │    │   Transaction   │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

## Database Schema

### Tables Overview

#### `spot_prices`
Stores historical price data for token pairs across different chains.

```sql
CREATE TABLE spot_prices (
    id BIGSERIAL PRIMARY KEY,
    token_a_symbol VARCHAR(50) NOT NULL,
    token_b_symbol VARCHAR(50) NOT NULL,
    block_height BIGINT NOT NULL,
    min_price DOUBLE PRECISION NOT NULL,
    max_price DOUBLE PRECISION NOT NULL,
    min_pool_id VARCHAR(100) NOT NULL,
    max_pool_id VARCHAR(100) NOT NULL,
    chain VARCHAR(50) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
```

**Key Indexes**:
- `idx_spot_prices_chain_block` - Chain and block height queries
- `idx_spot_prices_min_pool_height` - Pool-specific lookups
- `idx_spot_prices_max_pool_height` - Pool-specific lookups

#### `signals`
Stores detected arbitrage opportunities with full trade details.

```sql
CREATE TABLE signals (
    id BIGSERIAL PRIMARY KEY,
    slow_chain VARCHAR(50) NOT NULL,
    slow_height BIGINT NOT NULL,
    slow_pool_id VARCHAR(100) NOT NULL,
    fast_chain VARCHAR(50) NOT NULL,
    fast_height BIGINT NOT NULL,
    fast_pool_id VARCHAR(100) NOT NULL,
    -- ... swap details and profit calculations
    max_slippage_bps uint_bps NOT NULL,
    congestion_risk_discount_bps uint_bps NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
```

**Key Features**:
- Cross-chain arbitrage details
- Expected profit calculations
- Risk parameters (slippage, congestion)
- Complete swap path information

## Configuration Architecture

### Configuration Hierarchy

```
Environment Variables (Highest Priority)
        │
        ▼
kuma.yaml (Default Configuration)
        │  
        ▼
Built-in Defaults (Lowest Priority)
```

### Configuration Sections

#### Database Configuration
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
```

#### Blockchain Networks
```yaml
chains:
  - name: ethereum
    rpc_url: "https://ethereum-rpc.publicnode.com"
    tycho_url: "tycho-beta.propellerheads.xyz/"
    permit2_address: "0x000000000022d473030f116ddee9f6b43ac78ba3"
    private_key: "0x..."
```

#### Trading Strategies
```yaml
strategies:
  - token_a: UNI
    token_b: WETH  
    slow_chain: ethereum
    fast_chain: unichain
```

## Project Structure

```
kuma/
├── crates/
│   ├── cli/                 # Command-line interface
│   │   ├── src/
│   │   │   ├── main.rs     # Entry point
│   │   │   ├── cli.rs      # Command definitions
│   │   │   ├── dryrun.rs   # Simulation logic
│   │   │   ├── execute.rs  # Execution logic
│   │   │   ├── tokens.rs   # Token operations
│   │   │   └── permit.rs   # Permit2 handling
│   │   └── Cargo.toml
│   │
│   ├── core/               # Shared library
│   │   ├── src/
│   │   │   ├── lib.rs      # Module exports
│   │   │   ├── config.rs   # Configuration
│   │   │   ├── chain.rs    # Blockchain abstraction
│   │   │   ├── signals.rs  # Signal generation
│   │   │   ├── spot_prices.rs  # Price handling
│   │   │   ├── database/   # Database operations
│   │   │   │   ├── mod.rs
│   │   │   │   ├── signals.rs
│   │   │   │   └── spot_prices.rs
│   │   │   ├── strategy/   # Trading strategies
│   │   │   │   ├── mod.rs
│   │   │   │   ├── builder.rs
│   │   │   │   ├── precompute.rs
│   │   │   │   └── simulation.rs
│   │   │   ├── collector/  # Data collection
│   │   │   │   ├── mod.rs
│   │   │   │   └── builder.rs
│   │   │   └── state/      # State management
│   │   │       ├── mod.rs
│   │   │       ├── block.rs
│   │   │       └── pair.rs
│   │   └── Cargo.toml
│   │
│   ├── kumad/             # Daemon service
│   │   ├── src/
│   │   │   ├── main.rs    # Service entry point
│   │   │   ├── lib.rs     # Core daemon logic
│   │   │   ├── telemetry.rs  # Logging setup
│   │   │   └── strategy/  # Strategy execution
│   │   └── Cargo.toml
│   │
│   └── backend/           # API server
│       ├── src/
│       │   ├── main.rs    # Server entry point
│       │   ├── lib.rs     # Application logic
│       │   ├── models.rs  # Data models
│       │   ├── pair.rs    # Pair utilities
│       │   └── routes/    # HTTP endpoints
│       │       ├── mod.rs
│       │       ├── signals.rs
│       │       └── spot_prices.rs
│       └── Cargo.toml
│
├── webapp/                # Web interface
│   ├── src/
│   │   ├── app/           # Next.js app router
│   │   ├── components/    # React components
│   │   │   ├── signals/   # Signal components
│   │   │   ├── spot_prices/  # Price components
│   │   │   └── ui/        # UI components
│   │   └── lib/           # Utilities
│   ├── package.json
│   └── next.config.ts
│
├── migrations/            # Database migrations
├── docs/                  # Documentation
├── kuma.yaml             # Configuration file
├── justfile              # Command runner
├── docker-compose.yml    # Docker services
└── Cargo.toml           # Workspace definition
```

## Inter-Component Communication

### 1. CLI ↔ Core Library
- Direct function calls within the same process
- Shared configuration and data models
- Error propagation through Result types

### 2. Daemon ↔ Database
- SQLx for type-safe database operations
- Connection pooling for performance
- Async operations for non-blocking I/O

### 3. API Server ↔ Database
- SQLx with compile-time query verification
- Pagination for large result sets
- RESTful endpoints with JSON serialization

### 4. Web Interface ↔ API Server
- HTTP REST API calls
- TypeScript types for API responses
- Real-time updates via polling

### 5. Components ↔ Blockchain Networks
- Alloy library for Ethereum interactions
- Tycho protocol for simulation and execution
- WebSocket connections for real-time data

## Error Handling Strategy

### Error Types
- **Configuration Errors**: Invalid settings, missing files
- **Network Errors**: RPC failures, connection timeouts
- **Database Errors**: Connection issues, constraint violations
- **Trading Errors**: Insufficient balance, slippage exceeded
- **System Errors**: Out of memory, disk space issues

### Error Propagation
- `color-eyre` for rich error context
- `Result<T, E>` types throughout the codebase
- Graceful degradation where possible
- Detailed logging for debugging

## Performance Considerations

### Database Optimization
- Proper indexing on frequently queried columns
- Connection pooling to reduce overhead
- Prepared statements for repeated queries
- Pagination for large result sets

### Memory Management
- Rust's ownership system prevents memory leaks
- Careful use of collections and cloning
- Streaming for large data processing
- Connection pooling for resource efficiency

### Concurrency
- Tokio async runtime for non-blocking I/O
- Channel-based communication between components
- Proper use of Arc/Mutex for shared state
- Graceful shutdown handling

## Security Architecture

### Authentication & Authorization
- API key validation for external services
- Rate limiting on public endpoints
- Input validation and sanitization
- No direct database access from web interface

### Private Key Management
- Environment variable storage
- No hardcoded keys in source code
- Secure key derivation for deterministic wallets
- Hardware wallet support (future enhancement)

### Network Security
- HTTPS for all external communications
- Validated RPC endpoints
- Secure WebSocket connections
- Request/response validation

## Monitoring & Observability

### Logging
- Structured logging with tracing crate
- Multiple log levels (error, warn, info, debug, trace)
- Component-specific log filtering
- Centralized log aggregation in production

### Metrics
- Performance metrics collection
- Database query performance
- Trading success/failure rates
- System resource usage

### Health Checks
- Database connection health
- Blockchain RPC connectivity
- Service dependency status
- API endpoint availability

This architecture provides a solid foundation for reliable, scalable cross-chain arbitrage operations while maintaining security and observability throughout the system.