default:
  @just --list

set dotenv-load
set fallback

default_env := 'local'
copy-env type=default_env:
    cp {{ type }}.env.example .env

dev-webapp:
    cd webapp && npm run dev

# CLI commands
###################

generate-signals:
    cargo run -p kuma-cli -- \
    --token-a usdc --token-b weth \
    --chain-a ethereum --chain-b unichain \
    generate-signals

# Backend API server commands
##############################

# Run the API backend server
backend:
  exec cargo run --bin kuma-backend

# Test the API backend endpoints
backend-test block_height="19500000" page="1" page_size="10":
    curl "http://localhost:3000/spot_prices?block_height={{block_height}}&page={{page}}&page_size={{page_size}}"

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
    docker exec kuma-db-1 psql -U api_user -d api_db -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"
    sqlx migrate run --database-url "${DATABASE_URL:-postgres://api_user:password@localhost:5432/api_db}" --source "crates/backend/migrations"

# Run database migrations
db-migrate:
    sqlx migrate run --database-url "${DATABASE_URL:-postgres://api_user:password@localhost:5432/api_db}" --source "crates/backend/migrations"


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
    docker exec kuma-db-1 psql -U api_user -d api_db -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"
    sqlx migrate run --database-url "${DATABASE_URL:-postgres://api_user:password@localhost:5432/api_db}" --source "crates/backend/migrations"

# Run database migrations  
db-migrate:
    #!/usr/bin/env bash
    sqlx migrate run --database-url "${DATABASE_URL:-postgres://api_user:password@localhost:5432/api_db}" --source "crates/backend/migrations"

# Backend API server commands
##############################

# Run the API backend server
backend:
  exec cargo run --bin kuma-backend

# Test the API backend endpoints
backend-test block_height="19500000" page="1" page_size="10":
    curl "http://localhost:3000/spot_prices?block_height={{block_height}}&page={{page}}&page_size={{page_size}}"
