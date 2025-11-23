# kuma
![kuma](kuma.png)

Cross-chain arbitrage bot for [Tycho Community Extensions TAP-6](https://github.com/propeller-heads/tycho-x/blob/main/TAP-6.md).

[Proposal link](https://hackmd.io/qU6uAJ2BQe2pDwI2k9m_XA) (or in the [docs](docs/tap6-proposal.md) directory).

## Local Development
See [Prerequisites](#prerequisites)

### Quick Start
```bash
# Using Just commands (recommended)
just db-start      # Start PostgreSQL with Docker Compose
just db-migrate    # Run migrations (if available)
just backend       # Start the API server
```

###

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
**Reset database** (removes all data):
```bash
# Using Just commands (recommended)
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

### Prerequisites
- [Docker](https://docs.docker.com/get-docker/) and Docker Compose
- [Rust](https://rustup.rs/) (latest stable)
- [SQLx CLI](https://github.com/launchbadge/sqlx/tree/main/sqlx-cli)
- [Cargo SQLx Build Tool](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md#with-rust-toolchain)
