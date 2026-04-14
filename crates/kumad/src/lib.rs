//! Kuma daemon — the main binary crate for the cross-chain arbitrage bot.
//!
//! Exposes a single public entry point: [`Kuma::spawn`], which reads a [`kuma_core::config::Config`],
//! wires together all subsystems (collectors, strategies, execution worker), and returns a
//! [`Kuma`] handle that can be awaited or explicitly shut down.
//!
//! ## Internal modules
//!
//! | Module | Role |
//! |--------|------|
//! | [`kuma`](mod@kuma) | Orchestrator — starts and gracefully stops all subsystems |
//! | [`strategy`] | Strategy worker — slow/fast chain event loop, signal generation, DB writes |
//! | [`execution`] | Trade execution worker — promotes signals to on-chain trades |
//! | [`telemetry`] | `tracing` subscriber setup (logging + spans) |
//!
//! See `docs/signal-lifecycle.md` for the end-to-end data flow.

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
