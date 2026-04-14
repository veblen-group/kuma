# HTTP API

`kuma-backend` exposes a read-only JSON API for monitoring. All endpoints are served under the root path (no `/api` prefix at the Rust level — the Caddyfile strips `/api` before proxying).

## Base URL

| Environment | URL |
|-------------|-----|
| Production | `https://kuma.veblen.group/api` |
| Local | `http://localhost:8080` |

## Pagination

Most list endpoints accept:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `page` | int | 1 | Page number (1-indexed) |
| `page_size` | int | 10 | Results per page (capped at 100) |

Response envelope:

```json
{
  "data": [...],
  "page": 1,
  "page_size": 10,
  "total": 42
}
```

---

## GET /spot_prices

Returns paginated spot price rows for a token pair, ordered by `created_at DESC`.

### Query parameters

| Parameter | Required | Example | Description |
|-----------|----------|---------|-------------|
| `pair` | ✓ | `USDC-WETH` | `{token_a}-{token_b}` in strategy order |
| `chains` | | `ethereum,base` | Comma-separated chain filter |
| `page` / `page_size` | | | Pagination |

### Response item

```json
{
  "id": 1234,
  "created_at": "2026-04-13T10:00:00Z",
  "chain": "ethereum",
  "pair_token_a": "USDC",
  "pair_token_b": "WETH",
  "block_height": 21000000,
  "min_price": 2998.123456,
  "max_price": 3001.456789,
  "min_pool_id": "0xabc...",
  "max_pool_id": "0xdef..."
}
```

`min_price` / `max_price` are **quote per base** in the adjusted direction (see [price-direction.md](price-direction.md)). For USDC-WETH this is USDC per WETH ≈ 3000.

---

## GET /spot_prices/chart

Returns the latest spot prices for chart display (no pagination, larger result set). Used by the webapp price chart.

Same query parameters as `/spot_prices` except pagination.

---

## GET /signals

Returns paginated arbitrage signals ordered by `created_at DESC`.

### Query parameters

| Parameter | Required | Example |
|-----------|----------|---------|
| `pair` | ✓ | `USDC-WETH` |
| `page` / `page_size` | | |

### Response item

```json
{
  "id": 5678,
  "created_at": "2026-04-13T10:00:01Z",
  "slow_chain": "ethereum",
  "slow_height": 21000000,
  "slow_pool_id": "0x...",
  "slow_swap": { "token_in": "WETH", "token_out": "USDC", "amount_in": "...", "amount_out": "..." },
  "fast_chain": "unichain",
  "fast_height": 5000000,
  "fast_pool_id": "0x...",
  "fast_swap": { "token_in": "USDC", "token_out": "WETH", "amount_in": "...", "amount_out": "..." },
  "expected_profit": {
    "token_a": "USDC",
    "token_b": "WETH",
    "outcome": "profitable",
    ...
  },
  "slow_spot_prices": { ... },
  "fast_spot_prices": { ... }
}
```

---

## GET /signals/:id

Returns a single signal by ID with full detail including embedded spot prices.

---

## GET /trades

Returns paginated trade results ordered by `created_at DESC`.

### Response item

```json
{
  "id": 91,
  "created_at": "2026-04-13T10:00:02Z",
  "status": "successful",
  "signal": { ... },
  "slow_tx_hash": "0x...",
  "fast_tx_hash": "0x...",
  "realized_profit": {
    "total_usdc": "1234000",
    "surplus": ["500000", "0"],
    ...
  }
}
```

`status` is one of: `successful`, `failed_slow`, `failed_fast`.

---

## Running locally

```bash
just backend            # cargo run --bin kuma-backend
just backend-test       # curl spot_prices endpoint
```
