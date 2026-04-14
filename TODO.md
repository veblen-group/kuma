# Kuma Docs TODO

Ephemeral working document. Delete once all items are done.

---

## 1. Docs folder — new `.md` files to write

Each file lives in `docs/`. Use the existing docs (especially `tap6-proposal.md`,
`tycho-integration.md`, `trade-lifecycle.png`) as source material, and cross-reference
the relevant docstrings and module docs for accuracy.

| File | Content |
|------|---------|
| ✅ `docs/overview.md` | What is Kuma? Cross-chain arb bot summary, diagram ref (`high-level.png`), links to other docs |
| ✅ `docs/slow-fast-chain-model.md` | Why two chains, latency asymmetry, which chains are slow/fast and why, the 75% block-time deadline |
| ✅ `docs/signal-lifecycle.md` | End-to-end: slow precompute → fast signal → dedup → emit → trade execution → DB write. Ref `trade-lifecycle.png` |
| ✅ `docs/price-direction.md` | `token_a_b_adjusted_for_usdc()`, `spot_price(base, quote)` = quote-per-base, USDC-as-quote invariant, concrete examples (USDC/WETH → ~3000) |
| ✅ `docs/signal-profit.md` | `ExpectedProfit` vs `RealizedProfit`, slippage bps, congestion risk discount, gas cost in USDC, `same_outcome` dedup |
| ✅ `docs/collector.md` | ETH collector + Tycho collector → block multiplexer, lag tolerance, block alignment, `BlockStateStream` |
| ✅ `docs/configuration.md` | `kuma.yaml` schema: strategies, chains, binary search steps, slippage bps, profit flags (`ignore_gas_costs_in_profit`, etc.) |
| ✅ `docs/database.md` | Schema overview, fire-and-forget write pattern, spot price FK resolution via SQL CTE, repositories |
| ✅ `docs/api.md` | `kuma-backend` HTTP API: `GET /spot_prices`, `GET /signals`, `GET /trades` — params, response shapes |

---

## 2. Module/function docstrings to add

Priority-ordered. Check off as you go.

### High priority (load-bearing concepts, completely undocumented)

- [x] `crates/core/src/lib.rs` — crate-level `//!` doc: what kuma-core is, main entry points
- [x] `crates/kumad/src/kuma/mod.rs` — `//!` module doc on `Kuma` struct and `new()`/`run()`/`shutdown()` — this is the system overview in code form
- [x] `crates/core/src/signals/mod.rs` — `//!` module doc; explain `Direction::AtoB/BtoA`, `CrossChainSingleHop` signal shape
- [x] `crates/core/src/signals/profit.rs` — `//!` module doc on `ExpectedProfit`, `RealizedProfit`, discount chain
- [x] `crates/kumad/src/execution/mod.rs` — `//!` module doc on trade execution worker, one-at-a-time trade gate

### Medium priority (module context missing but fn docs exist)

- [x] `crates/core/src/state/pair.rs` — `//!` module doc: `Pair` (static config) vs `PairState` (live pool state)
- [x] `crates/core/src/state/block.rs` — `//!` module doc: `BlockState`, `BlockStateStream`, what gets streamed
- [x] `crates/core/src/state/mod.rs` — `//!` module doc: `PoolId`, state submodule map
- [x] `crates/core/src/chain.rs` — `//!` module doc: `Chain` struct, signer, metadata, slow vs fast role
- [x] `crates/core/src/config.rs` — `//!` module doc: config file schema, key knobs and their effects
- [x] `crates/core/src/trade.rs` — `//!` module doc: `Trade` = signal + two unsigned txs, `run()` semantics
- [x] `crates/core/src/database/mod.rs` — `//!` module doc: `Handle`, repository pattern, fire-and-forget writes

### Lower priority (thin modules or boilerplate)

- [ ] `crates/core/src/state/swap.rs` — `//!` module doc
- [ ] `crates/core/src/state/erc20.rs` — `//!` module doc
- [ ] `crates/core/src/state/balances.rs` — `//!` module doc + fn docs
- [ ] `crates/core/src/database/spot_prices.rs` — `//!` module doc
- [ ] `crates/core/src/database/signals.rs` — `//!` module doc
- [ ] `crates/core/src/database/trade.rs` — `//!` module doc
- [ ] `crates/core/src/strategy/builder.rs` — `//!` module doc
- [ ] `crates/core/src/collector/builder.rs` — `//!` module doc
- [ ] `crates/kumad/src/telemetry.rs` — `//!` module doc
- [ ] `crates/backend/src/routes/spot_prices.rs` — `//!` module doc
- [ ] `crates/backend/src/routes/signals.rs` — `//!` module doc
- [ ] `crates/backend/src/routes/trades.rs` — `//!` module doc
- [ ] `crates/backend/src/models.rs` — `//!` module doc
- [ ] `crates/backend/src/pair.rs` — `//!` module doc

---

## 3. Priority gaps (concepts, not just missing annotations)

These need both a docs `.md` file AND updated docstrings to be consistent:

1. **Slow/fast chain model** — core architectural concept, zero prose anywhere in the codebase
2. **Signal lifecycle / dedup** — `same_outcome` logic in `kumad/strategy/mod.rs` is subtle, not explained
3. **`Direction::AtoB/BtoA`** — has a `// TODO: rename to buy/sell?` comment; needs a decision + docs
4. **`ExpectedProfit` discount chain** — slippage → congestion → gas → USDC conversion; currently reverse-engineered from code
5. **Orchestrator startup order** — `Kuma::new()` wires collectors → strategies → execution; undocumented dependency graph
6. **Price direction** — ✓ done in `spot_prices.rs` + `state/pair.rs`; reference this from `docs/price-direction.md`

---

## 4. Generating and viewing docs locally

```bash
just docs             # builds kuma-core docs, opens browser
just docs kuma-core   # explicit (same as default)
just docs kumad       # scope to kumad binary crate
just docs kuma-backend
```

`cargo doc --no-deps` output lands in `target/doc/`. Key entry points:
- `target/doc/kuma_core/index.html`
- `target/doc/kumad/index.html`
- `target/doc/kuma_backend/index.html`

---

## 5. Hosting docs at kuma.veblen.group/docs

### Approach

Build rustdoc in CI/CD and serve the static output via Caddy — no new containers needed.

**Step 1 — Build step in Dockerfile or a separate `docs` image**

Add to the existing multi-stage `Dockerfile` (or a separate `Dockerfile.docs`):

```dockerfile
FROM rust:1.82 AS docs-builder
WORKDIR /app
COPY . .
RUN cargo doc --no-deps --workspace

FROM caddy:2-alpine AS docs
COPY --from=docs-builder /app/target/doc /srv/docs
```

**Step 2 — Add a `kuma-docs` service to `docker-compose.prod.yml`**

```yaml
kuma-docs:
  build:
    context: .
    dockerfile: Dockerfile.docs
  container_name: kuma-docs
  networks:
    - kuma-network
  restart: unless-stopped
```

**Step 3 — Add route in `Caddyfile`**

```
kuma.veblen.group {
    handle /docs/* {
        uri strip_prefix /docs
        reverse_proxy kuma-docs:80
    }

    handle /api/* {
        uri strip_prefix /api
        reverse_proxy kuma-backend:8080
    }

    handle {
        reverse_proxy kuma-webapp:3000
    }
}
```

Or, simpler if you don't want a separate container — mount the docs output
as a volume into the existing Caddy container and serve as static files:

```
handle /docs/* {
    root * /docs
    file_server
}
```

**Step 4 — Add `just` recipes**

```just
# Build rustdoc and copy output to docs-dist/ for deployment
docs-build:
    cargo doc --no-deps --workspace
    @echo "Docs built at target/doc/"

# Push docs to production (requires cloud-sql-proxy not needed, just scp)
docs-push zone="us-central1-c":
    gcloud compute scp --recurse target/doc kuma:/home/{{ remote_user }}/kuma/docs-dist --zone={{ zone }}
```

**Default URL:** `https://kuma.veblen.group/docs/kuma_core/index.html`

Consider adding a redirect from `/docs` → `/docs/kuma_core/index.html` in Caddy for convenience.
