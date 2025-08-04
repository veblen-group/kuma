# kuma
![kuma](kuma.png)

Cross-chain arbitrage bot for [Tycho Community Extensions TAP-6](https://github.com/propeller-heads/tycho-x/blob/main/TAP-6.md).

# API Server Database Integration

This crate provides a production-ready database integration layer for the Kuma API server using PostgreSQL and SQLx.

## Usage

### Configuration

The backend loads configuration from `kuma.yaml` in the workspace root. Specifically:

```yaml
database:
  url: "postgres://api_user:password@localhost:5432/api_db"
  max_connections: 10
  connection_timeout_secs: 30
  idle_timeout_secs: 600

server:
  host: "0.0.0.0"
  port: 3000
```

Environment variables with `KUMA_` prefix override config file values.

## Database Schema

Run the migrations to set up the required tables:

```sql
-- See migrations/001_initial.sql for the complete schema
```

The schema includes tables for:
- `spot_prices`: Token pair spot price data indexed by pool and block height
- `signals`: Cross-chain arbitrage opportunities with full swap details

## Local Development

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) and Docker Compose
- [Rust](https://rustup.rs/) (latest stable)
- [SQLx CLI](https://github.com/launchbadge/sqlx/tree/main/sqlx-cli)
- [Cargo SQLx Build Tool](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md#with-rust-toolchain)

### Quick Start

```bash
# Using Just commands (recommended)
just db-start      # Start PostgreSQL with Docker Compose
just db-migrate    # Run migrations (if available)
just backend       # Start the API server
just backend-test  # Test the API

# Or manually:
# 1. Start PostgreSQL
cd crates/backend && docker-compose up -d

# 2. Run migrations (if available)
sqlx migrate run --database-url "postgres://api_user:password@localhost:5432/api_db"

# 3. Start the server from backend directory
cd crates/backend
cargo run

# 4. Test the API
curl "http://localhost:3000/spot_prices?block_height=19500000&page=1&page_size=10"
```
### Database Management

**Reset database** (removes all data):
```bash
docker exec -i kuma-postgres psql -U api_user -d api_db -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"
sqlx migrate run --database-url $DATABASE_URL
```

### Compile-Time Query Validation
When compiling the backend, SQLx will validate all queries at compile time. This ensures that any SQL errors are caught early and prevents runtime errors.

Queries that have been modified need to be recompiled with SQLx CLI so they can be checked without requiring a DB connection in build time (["offline mode"](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md#enable-building-in-offline-mode-with-query)):

```bash
cargo sqlx prepare --database-url $DATABASE_URL

# Or more simply, with the just command from the workspace root:
just db-prepare
```

If the database schema is modified, you may need to reset the database and run migrations again before recompiling with the SQLx CLI.
