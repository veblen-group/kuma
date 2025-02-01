use std::time::Duration;

use color_eyre::eyre;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument};

use crate::config::Config;

pub(super) struct Kuma {
    shutdown_token: CancellationToken,
}

impl Kuma {
    pub(super) fn new(_cfg: Config, shutdown_token: CancellationToken) -> eyre::Result<Self> {
        // TODO: initialize components here
        Ok(Self { shutdown_token })
    }

    pub(super) async fn run(self) -> eyre::Result<()> {
        let timer = tokio::time::sleep(Duration::from_secs(3));
        tokio::pin!(timer);

        let reason: eyre::Result<&str> = {
            loop {
                select! {
                    biased;

                    () = self.shutdown_token.cancelled() => break Ok("received shutdown signal"),

                    _ = &mut timer => {
                        // info!("timer tick");
                        // self.shutdown_token.cancel();
                    }

                    // TODO: add components here
                }
            }
        };

        Ok(self.shutdown(reason).await)
    }

    #[instrument(skip_all)]
    async fn shutdown(self, reason: eyre::Result<&'static str>) {
        const WAIT_BEFORE_ABORT: Duration = Duration::from_secs(25);

        // trigger the shutdown token in case it wasn't triggered yet
        self.shutdown_token.cancel();

        let message = format!(
            "waiting {} for all subtasks to shutdown before aborting",
            humantime::format_duration(WAIT_BEFORE_ABORT)
        );
        match &reason {
            Ok(reason) => info!(%reason, message),
            Err(reason) => error!(%reason, message),
        };

        // TODO: handle running subtasks here
    }
}
