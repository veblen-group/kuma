//! Kuma daemon — the main binary crate for the cross-chain arbitrage bot.
//!
//! ## Binary entry point
//!
//! `main.rs` drives the binary in four steps:
//!
//! 1. **Config** — [`Config::load()`](kuma_core::config::Config::load) reads `kuma.yaml` from the working
//!    directory and merges any `KUMA_*` environment variable overrides (via Figment).
//!    If loading fails the process exits immediately with a printed error.
//! 2. **Telemetry** — [`telemetry::get_subscriber`] builds a `tracing` subscriber and
//!    [`telemetry::init_subscriber`] registers it globally. Log level is controlled by
//!    `RUST_LOG`; set `KUMA_SPLIT_VIEW=1` for a two-stream mode that routes INFO to stdout
//!    and verbose/span output to stderr (useful with `tmux` split-pane monitoring).
//! 3. **Service** — [`Kuma::spawn`] wires up all subsystems and spawns the runtime task.
//! 4. **Shutdown** — `select!` on SIGTERM or a service error, then calls [`Kuma::shutdown`]
//!    which cancels the shared `CancellationToken` and drains all workers.
//!
//! ## Modules
//!
//! ### [`kuma`](mod@kuma) — orchestrator
//!
//! [`kuma::Kuma`] is the top-level coordinator. `Kuma::new` reads the config and wires:
//! - One **collector set** per chain (ETH RPC + Tycho + block multiplexer), reusing the
//!   same collector when multiple strategies share a chain.
//! - One **strategy worker** per configured `(token_a, token_b, slow_chain, fast_chain)` pair,
//!   each receiving `BlockStateStream`s for its slow and fast chains.
//! - One **trade execution worker** subscribing to signals from all strategy workers.
//!
//! `Kuma::run` polls all subsystem futures concurrently; the first to complete (success or
//! error) triggers shutdown. `Kuma::shutdown` cancels the `CancellationToken` and sequentially
//! awaits each worker with a 25-second deadline before aborting.
//!
//! ### [`strategy`] — strategy worker
//!
//! One worker per trading pair. The event loop (`Worker::run`) uses a biased `select!` over:
//! - **Slow block** → call `strategy.try_precompute`, write slow spot prices to DB, set 75%
//!   submission deadline timer.
//! - **Fast block** → call `strategy.generate_signal`, write fast spot prices; if profitable
//!   and not deduped by `same_outcome`, queue signal and write to DB.
//! - **Deadline fires** → emit `curr_signal` to the execution worker via `mpsc`.
//! - **DB write completions** → drain `FuturesUnordered`, log errors.
//!
//! See [`kuma_core::strategy`] for the signal generation logic and
//! `docs/signal-lifecycle.md` for the full flow.
//!
//! ### [`execution`] — trade execution worker
//!
//! A single worker receiving signals from all strategy workers via merged `select_all`. Only
//! one trade runs at a time — if a signal arrives while a trade is in-flight it is dropped.
//! On each signal:
//! 1. `signal.try_promote()` encodes both transaction legs into `UnsignedTransaction`s.
//! 2. `trade.run()` executes sequentially: slow chain first, fast chain only on confirmation.
//! 3. The `TradeResult` is written to the database asynchronously.
//!
//! ### [`telemetry`] — tracing setup
//!
//! Builds a layered `tracing_subscriber` with noise-filtered targets (Tycho internals, Alloy
//! transports, sqlx held at `WARN`). Supports an optional split-view mode for terminal
//! monitoring via `KUMA_SPLIT_VIEW=1`.

// Links to private modules are intentional — kumad is a binary crate and docs are always
// built with --document-private-items so the links resolve correctly.
#![allow(rustdoc::private_intra_doc_links)]

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use color_eyre::eyre::{self, Context as _};
use kuma_core::config::Config;
use tokio::task::{JoinError, JoinHandle};
use tokio_util::sync::CancellationToken;

mod execution;
mod kuma;
mod strategy;
pub mod telemetry;

/// The [`Kuma`] service returned by [`Kuma::spawn`].
pub struct Kuma {
    shutdown_token: CancellationToken,
    task: Option<JoinHandle<eyre::Result<()>>>,
}

impl Kuma {
    /// Spawns the [`Kuma`] service.
    ///
    /// # Errors
    /// Returns an error if Kuma cannot be initialized.
    pub fn spawn(cfg: Config) -> eyre::Result<Self> {
        let shutdown_token = CancellationToken::new();
        let inner = kuma::Kuma::new(cfg, shutdown_token.child_token())?;
        let task = tokio::spawn(inner.run());

        Ok(Self {
            shutdown_token,
            task: Some(task),
        })
    }

    /// Shuts down Kuma, in turn waiting for its components to shut down.
    ///
    /// # Errors
    /// Returns an error if an error occured during shutdown.
    ///
    /// # Panics
    /// Panics if called twice
    pub async fn shutdown(mut self) -> eyre::Result<()> {
        self.shutdown_token.cancel();
        flatten_join_result(
            self.task
                .take()
                .expect("shutdown must only be called one")
                .await,
        )
    }
}

impl Future for Kuma {
    type Output = eyre::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use futures::future::FutureExt as _;

        let task = self
            .task
            .as_mut()
            .expect("kuma must not be polled after completion");
        task.poll_unpin(cx).map(flatten_join_result)
    }
}

fn flatten_join_result<T>(res: Result<eyre::Result<T>, JoinError>) -> eyre::Result<T> {
    match res {
        Ok(Ok(res)) => Ok(res),
        Ok(Err(e)) => Err(e).wrap_err("task returned with error"),
        Err(e) => Err(e).wrap_err("task panicked"),
    }
}
