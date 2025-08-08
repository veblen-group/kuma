default:
  @just --list

set fallback

dev-webapp:
    cd webapp && npm run dev

# CLI commands
###################

generate-signals token-a="usdc" token-b="weth" slow-chain="ethereum" fast-chain="unichain":
    cargo run -p kuma-cli generate-signals \
    --token-a {{token-a}} --token-b {{token-b}} \
    --slow-chain {{slow-chain}} --fast-chain {{fast-chain}} \

get-tokens chain="ethereum":
    cargo run -p kuma-cli tokens --chain {{chain}}

init-permit2:
    cargo run -p kuma-cli init-permit2

# Bot commands

kumad:
    cargo run -p kumad

# Backend API server commands
##############################

# Run the API backend server
backend:
  exec cargo run --bin kuma-backend

# Test the API backend endpoints
backend-test endpoint="spot_prices" pair="USDC-WETH" page="1" page_size="10":
    curl "http://localhost:8080/{{endpoint}}?pair={{pair}}&page={{page}}&page_size={{page_size}}"

# Database commands
###################

# Start PostgreSQL database with Docker Compose
db-start:
  docker-compose up -d

# Stop PostgreSQL database
db-stop:
  docker-compose down

# Reset database (removes all data)
db-reset:
    #!/usr/bin/env bash
    docker exec kuma-db psql -U api_user -d api_db -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"
    sqlx migrate run --database-url "${DATABASE_URL:-postgres://api_user:password@localhost:5432/api_db}" --source "migrations"

# Run database migrations
db-migrate:
    sqlx migrate run --database-url "${DATABASE_URL:-postgres://api_user:password@localhost:5432/api_db}" --source "migrations"

# Compile-time checks for postgres queries
db-prepare:
    cargo sqlx prepare --workspace --database-url "${DATABASE_URL:-postgres://api_user:password@localhost:5432/api_db}"

default_lang := 'all'
# Format
#########
[doc("
Can format 'rust', 'toml', 'proto', or 'all'. Defaults to all.
")]
fmt lang=default_lang:
  @just _fmt-{{lang}}

_fmt-all:
  @just _fmt-rust
  @just _fmt-toml

[no-exit-message]
_fmt-rust:
  just _lint-rust-fmt
  just _lint-rust-clippy

[no-exit-message]
_lint-rust-fmt:
  cargo +nightly fmt --all -- --check

[no-exit-message]
_lint-rust-clippy:
  cargo clippy --version
  cargo clippy --all-targets --all-features \
          -- --warn clippy::pedantic --warn clippy::arithmetic-side-effects \
          --warn clippy::allow_attributes --warn clippy::allow_attributes_without_reason \
          --deny warnings

[no-exit-message]
_fmt-toml:
  taplo format --check
