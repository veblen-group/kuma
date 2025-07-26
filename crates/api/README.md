# API Server Database Integration

This crate provides a production-ready database integration layer for the Kuma API server using PostgreSQL and SQLx.

## Architecture

The database integration follows the Builder/Handle/Worker pattern:

- **DatabaseBuilder**: Configures and initializes the database connection
- **DatabaseWorker**: Background task managing connection health and monitoring  
- **DatabaseHandle**: Provides access to repositories and connection pool
- **Repository pattern**: Clean separation of database operations for different entities

## Usage

### Basic Setup

```rust
use api_server::database::{DatabaseBuilder, DatabaseConfig};

#[tokio::main]
async fn main() -> Result<()> {
    let config = DatabaseConfig::default();
    let (worker, handle) = DatabaseBuilder::new()
        .with_config(config)
        .build()
        .await?;

    // Start the database worker
    let worker_task = tokio::spawn(async move {
        worker.run().await
    });

    // Use the handle in your application state
    let state = AppState { db: handle };
    
    Ok(())
}
```

### Repository Usage

```rust
// Get repositories from the handle
let pair_repo = handle.pair_price_repository();
let signal_repo = handle.arbitrage_signal_repository();

// Insert a pair price
pair_repo.insert(&pair_price).await?;

// Query recent arbitrage signals
let signals = signal_repo.get_recent(10).await?;
```

### Environment Configuration

Set the `DATABASE_URL` environment variable:

```bash
export DATABASE_URL="postgres://user:password@localhost/database"
```

## Database Schema

Run the migrations to set up the required tables:

```sql
-- See migrations/001_initial.sql for the complete schema
```

The schema includes tables for:
- `pair_prices`: Token pair price data indexed by pool and block height
- `arbitrage_signals`: Cross-chain arbitrage opportunities with full swap details

## Features

- **Connection Pooling**: Configurable connection pool with health monitoring
- **Health Checks**: Background health monitoring with configurable intervals
- **Graceful Shutdown**: Proper cleanup of connections and background tasks
- **Repository Pattern**: Clean separation of concerns for database operations
- **Instrumentation**: Full tracing support for observability
- **Error Handling**: Comprehensive error handling with `color_eyre`

## Local Development

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) and Docker Compose
- [Rust](https://rustup.rs/) (latest stable)
- [SQLx CLI](https://github.com/launchbadge/sqlx/tree/main/sqlx-cli)

### Quick Start

1. **Install SQLx CLI** (if not already installed):
   ```bash
   cargo install sqlx-cli --no-default-features --features postgres
   ```

2. **Start PostgreSQL with Docker**:
   ```bash
   # From the project root, create a docker-compose.yml if it doesn't exist
   docker run --name kuma-postgres \
     -e POSTGRES_PASSWORD=password \
     -e POSTGRES_USER=api_user \
     -e POSTGRES_DB=api_db \
     -p 5432:5432 \
     -d postgres:15
   ```

   Or use Docker Compose (create `docker-compose.yml` in project root):
   ```yaml
   version: '3.8'
   services:
     postgres:
       image: postgres:15
       container_name: kuma-postgres
       environment:
         POSTGRES_DB: api_db
         POSTGRES_USER: api_user
         POSTGRES_PASSWORD: password
       ports:
         - "5432:5432"
       volumes:
         - postgres_data:/var/lib/postgresql/data
   
   volumes:
     postgres_data:
   ```

   Then run:
   ```bash
   docker-compose up -d
   ```

3. **Set Environment Variables**:
   ```bash
   export DATABASE_URL="postgres://api_user:password@localhost:5432/api_db"
   ```

4. **Run Database Migrations**:
   ```bash
   # From the crates/api directory
   cd crates/api
   sqlx migrate run --database-url $DATABASE_URL
   ```

   This will create tables and populate them with mock data from:
   - `migrations/001_initial.sql` - Creates tables and indexes
   - `migrations/002_seed_mock_data.sql` - Adds mock spot price data
   - `migrations/003_seed_arbitrage_signals.sql` - Adds mock arbitrage signal data

5. **Start the API Server**:
   ```bash
   cargo run
   ```

   The server will start at `http://localhost:3000`

### API Endpoints

Once running, you can test the endpoints:

```bash
# Get all spot prices for block 19500000
curl "http://localhost:3000/spot_prices?block_height=19500000"

# Get WETH-USDC pair prices for block 19500000
curl "http://localhost:3000/spot_prices?block_height=19500000&pair=WETH-USDC"

# Get arbitrage signals for block 19500000
curl "http://localhost:3000/signals?block_height=19500000"

# Get arbitrage signals with pagination
curl "http://localhost:3000/signals?block_height=19500000&limit=2&offset=1"
```

### Database Management

**Reset database** (removes all data):
```bash
docker exec -i kuma-postgres psql -U api_user -d api_db -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"
sqlx migrate run --database-url $DATABASE_URL
```

**View data**:
```bash
# Connect to database
docker exec -it kuma-postgres psql -U api_user -d api_db

# Check tables
\dt

# Query data
SELECT * FROM pair_prices LIMIT 5;
SELECT * FROM arbitrage_signals LIMIT 5;
```

**Stop services**:
```bash
# Docker Compose
docker-compose down

# Or single container
docker stop kuma-postgres
docker rm kuma-postgres
```

### Development Workflow

1. Make code changes
2. Run tests: `cargo test`
3. Check compilation: `cargo check` 
4. Run server: `cargo run`
5. Test endpoints with curl or your preferred HTTP client

### Troubleshooting

**Connection refused**:
- Ensure PostgreSQL container is running: `docker ps`
- Check DATABASE_URL is correctly set
- Verify port 5432 is not used by another service

**Migration errors**:
- Ensure you're in the `crates/api` directory when running migrations
- Check database connection with: `sqlx database create --database-url $DATABASE_URL`

**Permission errors**:
- Ensure Docker daemon is running
- Check user permissions for Docker commands

## Testing

The integration includes comprehensive tests:

```bash
cargo test --all-features
```

Tests cover:
- Builder configuration and validation
- Property-based testing of configuration parameters
- Database handle lifecycle management
- Background worker behavior
- Route query parameter deserialization