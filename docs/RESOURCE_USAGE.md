---
title: Resource Usage Estimates
description: Runtime memory and CPU estimates for kumad and kuma-backend.
updated: 2026-04-13
---

# Resource Usage Estimates

Runtime resource analysis for the kuma monorepo services. All estimates assume
**1 trading pair across 2 chains** with `binary_search_steps = 8192` (the
production default in `kuma.yaml`).

---

## kumad

The main arbitrage daemon. For a single pair on 2 chains it spawns **9 tokio
tasks**:

| Task               | Count | Purpose                              |
|--------------------|-------|--------------------------------------|
| Supervisor         | 1     | Awaits all worker handles            |
| Eth Collector      | 2     | 1 per chain -- WebSocket block subscription |
| Tycho Collector    | 2     | 1 per chain -- pool state stream      |
| Block Collector    | 2     | 1 per chain -- merges eth + tycho     |
| Strategy Worker    | 1     | Signal generation                    |
| Execution Worker   | 1     | Trade execution                      |

### Memory

| Component                         | Estimate   | Notes                                                        |
|-----------------------------------|------------|--------------------------------------------------------------|
| Tycho pool state (`BlockSim`)     | 10-40 MB   | Hundreds-thousands of DEX pools across 2 chains, filtered by TVL threshold. V2 pools ~64 bytes each, V3 pools several KB each (tick data). |
| Precomputes (`PoolSteps`)         | 14-24 MB   | 8192 steps x 2 directions x ~300 bytes/Swap x 3-5 pools     |
| Tokio runtime + binary            | 15-25 MB   | Async runtime overhead, static binary sections               |
| Watch channels, buffers, misc     | ~5 MB      | Single-slot watch channels (Arc-shared), token balances (<1 KB per chain) |
| **Total**                         | **45-95 MB** |                                                            |

#### Precompute sizing detail

Each `strategy::Swap` struct:
- `token_in: Token` (~80 bytes) + `token_out: Token` (~80 bytes)
- `amount_in: BigUint` (~48 bytes) + `amount_out: BigUint` (~48 bytes)
- `gas_cost: BigUint` (~24 bytes)
- **~300 bytes per Swap** (with allocation overhead)

Per pool: `8192 steps x 2 directions x 300 bytes = ~4.8 MB`
For 3-5 matching pools on the slow chain: **~14-24 MB**

Precomputes are regenerated every slow block and the old allocation is dropped.

### CPU

| Event             | Frequency          | Work                                              | Duration      |
|-------------------|--------------------|----------------------------------------------------|---------------|
| Slow block        | Every 2-12s (chain-dependent) | 8192 x 2 x N pools AMM simulations (precompute)   | 50-500 ms     |
| Fast block        | Every 2s           | ~13 fast-chain sims (binary search over precomputes) + profit calc | < 1 ms     |
| Trade execution   | Infrequent         | ABI encoding + EIP712 signing                      | ~1 ms         |

**Sustained CPU:**
- With 12s slow blocks (Ethereum): < 5% of 1 core
- With 2s slow blocks (L2-to-L2): ~5-25% of 1 core

### Network

| Connection Type              | Count | Persistence |
|------------------------------|-------|-------------|
| WebSocket (chain RPC)        | 2     | Persistent  |
| Tycho protocol stream        | 2     | Persistent  |
| HTTP RPC (trade execution)   | 2/trade | Ephemeral |

### Database

- **Writes:** ~1-1.5 rows/sec steady state (spot prices + occasional signals/trades)
- **Reads:** None

### Scaling notes

Adding a **second pair on the same chains** reuses the existing collectors
(deduplicated by chain). Adds 1 strategy task, 1 set of precomputes (~14-24 MB),
and 1 additional `BlockStateStream` per chain. The execution worker is shared.

Adding a pair on a **new chain** adds 3 collector tasks, 1 WebSocket, 1 Tycho
stream, plus the full `BlockSim` for that chain (10-20 MB).

---

## kuma-backend

A stateless Axum HTTP server that proxies paginated read queries to PostgreSQL.

### Memory

| Component                     | Estimate     | Notes                                      |
|-------------------------------|--------------|--------------------------------------------|
| Axum runtime                  | ~5-10 MB     | Single tokio runtime, no thread pools      |
| PgPool (10 connections)       | ~5-10 MB     | TCP sockets + protocol buffers             |
| Token config map              | < 1 MB       | ~5 tokens x ~3 chains, in-memory           |
| Per-request allocations       | 100-200 KB   | Bounded by `page_size` max of 100 rows     |
| **Total**                     | **15-30 MB** |                                            |

### CPU

Effectively zero. All work is I/O-bound:
- Parse HTTP request
- Execute 2-6 SQL queries (paginated `COUNT(*)` + `SELECT ... LIMIT/OFFSET`)
- Serialize JSON response

No simulation, no cryptographic operations, no heavy computation.

### Database

- **Pool:** 10 max connections, lazy initialization
- **Reads:** 2-6 queries per request (count + data, via `tokio::join!`)
- **Note:** `/trades` endpoint has an N+1 query pattern -- each trade row triggers
  a sequential `get_by_id` signal lookup. Worst case: ~306 queries for a full
  page of combined trades.

---

## PostgreSQL (Cloud SQL)

Hosted on GCP Cloud SQL (db-f1-micro instance).

| Resource         | Estimate           | Notes                                          |
|------------------|--------------------|-------------------------------------------------|
| Instance tier    | db-f1-micro        | Shared vCPU, 614 MB RAM                         |
| Disk             | 10 GB SSD          | Grows ~50-100 MB/week from spot_prices writes    |
| Connections      | Up to 20           | 10 from backend + 10 from kumad (pool max)       |
| Backups          | Daily automatic     | 7-day retention, included free                   |

---

## Frontend (Next.js)

| Resource | Estimate     | Notes                                            |
|----------|--------------|--------------------------------------------------|
| RAM      | 80-120 MB    | Node.js standalone server, SSR                   |
| CPU      | Negligible   | Serves pre-rendered pages, React Query polls every 5 min |

---

## Total (VM services only, excluding Cloud SQL)

| Component         | RAM           | CPU (sustained)         |
|-------------------|---------------|-------------------------|
| kumad             | 45-95 MB      | < 5% (12s blocks) / ~25% (2s blocks) |
| kuma-backend      | 15-30 MB      | ~0%                     |
| Frontend          | 80-120 MB     | ~0%                     |
| Cloud SQL Proxy   | ~20 MB        | ~0%                     |
| Docker + OS       | ~200 MB       | --                      |
| **Total**         | **~360-465 MB** | **< 30% of 2 vCPU**  |

### VM recommendation

A **GCP e2-small** (2 vCPU, 2 GB RAM) provides ~1.6 GB usable after OS overhead,
leaving ~1.1 GB headroom above the worst-case estimate. This is sufficient for
running all services from pre-built Docker images (no compilation on the VM).

Building Rust binaries requires significantly more RAM (8+ GB recommended) and
should be done in CI or locally, not on the VM.
