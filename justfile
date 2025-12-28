default:
  @just --list

set fallback

# CLI commands
###################
generate-signal token-a="usdc" token-b="weth" slow-chain="ethereum" fast-chain="unichain":
    cargo run -p kuma-cli generate-signals \
    --token-a {{token-a}} --token-b {{token-b}} \
    --slow-chain {{slow-chain}} --fast-chain {{fast-chain}} \

dry-run input="signal.json" output="./trade.json":
    echo "TODO"

execute-trade input="trade.json":
    echo "TODO"

get-tokens chain="ethereum":
    cargo run -p kuma-cli tokens --chain {{chain}}

init-permit2:
    cargo run -p kuma-cli init-permit2

# kumad
####################
kumad:
    cargo run -p kumad

kumad-split:
    ./run_split.sh

kumad-start:
  docker compose --profile kumad up -d

kumad-init:
    docker compose --profile kumad --profile init up -d

kumad-stop:
    docker compose --profile kumad down

# Webapp
###################
# Run webapp in dev mode
webapp-dev:
    cd webapp && npm run dev

# Build the Next.js app
webapp-build:
    cd webapp && npm run build

# Start the webapp
webapp-start:
    cd webapp && npm run start

# Database
#####################
# Start PostgreSQL database with Docker Compose and run migrations
db-start:
  docker compose --profile db up -d

# Stop PostgreSQL database with Docker Compose
db-stop:
  docker compose --profile db down

# Reset database (removes all data)
db-reset:
    #!/usr/bin/env bash
    docker exec kuma-db psql -U api_user -d api_db -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"
    sqlx migrate run --database-url "${DATABASE_URL:-postgres://api_user:password@localhost:5432/api_db}" --source "migrations" --target-version "001"

# Run database migrations
db-migrate-test:
    sqlx migrate run --database-url "${DATABASE_URL:-postgres://api_user:password@localhost:5432/api_db}" --source "migrations"

# Compile-time checks for postgres queries
db-prepare:
    cargo sqlx prepare --workspace --database-url "${DATABASE_URL:-postgres://api_user:password@localhost:5432/api_db}"


# Backend API server commands
##############################
# Run the API backend server
backend:
  cargo run --bin kuma-backend

# Test the API backend endpoints
backend-test endpoint="spot_prices" pair="USDC-WETH" page="1" page_size="10":
    curl "http://localhost:8080/{{endpoint}}?pair={{pair}}&page={{page}}&page_size={{page_size}}"

# Docker commands
##################
# Build specific binary images
docker-build binary="kumad" tag="kumad" version="latest":
  docker build --build-arg BINARY={{binary}} -t {{tag}}:{{version}} .

docker-build-webapp tag="webapp" version="latest":
  cd webapp && docker build -t {{tag}}:{{version}} .

docker-build-backend tag="backend" version="latest":
  just docker-build kuma-backend {{tag}} {{version}}

docker-build-all version="latest":
  just docker-build kumad kumad {{version}}
  just docker-build-backend backend {{version}}
  just docker-build-webapp webapp {{version}}

# Start all services including daemon, database, backend, and webapp
docker-run:
	docker compose --profile all up -d

# Stop all services
docker-stop:
  docker-compose --profile all down

# Linting & formatting
##################
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
