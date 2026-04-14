---
title: Database
description: PostgreSQL persistence layer — schema, write patterns, and repositories.
updated: 2026-04-13
---

# Database

Kuma uses PostgreSQL for persisting spot prices, signals, and trade results for monitoring and accounting.

## Schema

See `migrations/` and `schema.sql` for the authoritative schema. Key tables:

| Table | Contents |
|-------|----------|
| `spot_prices` | One row per (chain, pair, block) — min/max price and pool IDs |
| `signals` | One row per generated arbitrage signal with all swap parameters and expected profit |
| `trades` | One row per executed trade with receipts and realized profit |

## Write pattern — fire and forget

All database writes in the strategy and execution workers use `FuturesUnordered` to run writes concurrently without blocking the main event loop:

```rust
db_writes.push(async move { repo.write_fast_spot_prices(prices).await }.boxed());
```

Errors are logged (`error!`) but never propagate — a failed DB write does not cancel a trade or stop signal generation. The `db_writes.next()` arm in the `select!` loop drains completions.

## Repositories

`crates/core/src/database/mod.rs` — `Handle` creates short-lived repository objects:

```rust
let repo = db.spot_price_repository();  // SpotPriceRepository
let repo = db.signal_repository();      // SignalRepository
let repo = db.trade_repository();       // TradeRepository
```

Each repository clones an `Arc<PgPool>` — no connection ownership issues.

## Signal FK resolution via SQL CTE

Signals reference spot prices by foreign key. The insert query resolves these FKs at write time via a CTE that looks up `spot_prices` rows by `(chain, pair, block_height)`:

```sql
WITH slow_price AS (
    SELECT id FROM spot_prices
    WHERE chain = $slow_chain AND ... AND block_height = $slow_height
    LIMIT 1
)
INSERT INTO signals (..., slow_spot_price_id)
SELECT ..., slow_price.id FROM slow_price
```

This means spot-price writes and signal writes can be fired concurrently with no coordination — the CTE does the join at insert time.

## Connection pooling

`sqlx::PgPoolOptions` with:
- `max_connections` — configurable, default 10
- `acquire_timeout` — configurable, default 30 s
- `idle_timeout` — configurable, default 600 s
- `connect_lazy` — pool doesn't open connections until first use

## Local setup

```bash
just db-start          # start PostgreSQL via Docker Compose
just db-migrate-test   # run migrations + test seed data
just db-reset          # drop and recreate schema (destructive)
```
