use std::time::Duration;

use color_eyre::eyre::{self, Context};
use num_bigint::BigUint;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument};

use crate::{config::Config, strategy};
use kuma_core::{chain::Chain, collector, state::pair::Pair};

pub(super) struct Kuma {
    shutdown_token: CancellationToken,
    // TODO: do this for all configured chains
    slow_collector: collector::Handle,
    fast_collector: collector::Handle,
    strategy_handle: strategy::Handle,
}

impl Kuma {
    pub(super) fn new(cfg: Config, shutdown_token: CancellationToken) -> eyre::Result<Self> {
        // TODO: these should be methods on config instead of utils in core
        let (slow_chain, fast_chain) = Kuma::get_slow_fast_chains(&cfg);
        let (slow_pair, fast_pair) = (
            Kuma::get_pair(&cfg, &slow_chain),
            Kuma::get_pair(&cfg, &fast_chain),
        );

        let kuma_core::config::Config {
            tokens,
            chains,
            tycho_api_key,
            add_tvl_threshold,
            remove_tvl_threshold,
            congestion_risk_discount_bps,
            max_slippage_bps,
            binary_search_steps,
        } = cfg.core;

        let slow_collector = collector::Builder {
            chain: slow_chain,
            tycho_url: tycho_api_key,
            api_key: todo!(),
            tokens: todo!(),
            add_tvl_threshold: todo!(),
            remove_tvl_threshold: todo!(),
        }
        .build()
        .wrap_err("failed to start tycho collector for chain: {slow_chain:?}")?;
        let fast_collector = collector::Builder {
            chain: slow_chain,
            tycho_url: todo!(),
            api_key: todo!(),
            tokens: todo!(),
            add_tvl_threshold,
            remove_tvl_threshold,
        }
        .build()
        .wrap_err("failed to start tycho collector for chain: {fast_chain:?}")?;

        let slow_stream = slow_collector.get_pair_state_stream(&slow_pair);
        let fast_stream = fast_collector.get_pair_state_stream(&fast_pair);

        // TODO: init from config
        let strategy_handle = strategy::Builder {
            slow_pair: todo!(),
            slow_chain,
            fast_pair: todo!(),
            fast_chain,
            // TODO: use the helper methods for this
            slow_inventory: (BigUint::from(1000u64), BigUint::from(1000u64)),
            fast_inventory: (BigUint::from(1000u64), BigUint::from(1000u64)),
            binary_search_steps: 10,          // TODO: from config
            max_slippage_bps: 50,             // TODO: from config
            congestion_risk_discount_bps: 25, // TODO: from config
            slow_stream,
            fast_stream,
            slow_block_time_ms: 12000, // TODO: from config (12s for Ethereum)
            signal_buffer_size: 100,   // TODO: from config
        }
        .build()?;

        Ok(Self {
            shutdown_token,
            slow_collector,
            fast_collector,
            strategy_handle,
        })
    }

    pub(super) async fn run(mut self) -> eyre::Result<()> {
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

                    // Handle strategy worker completion
                    result = &mut self.strategy_handle => {
                        match result {
                            Ok(()) => break Ok("strategy worker completed"),
                            Err(e) => break Err(e),
                        }
                    }
                }
            }
        };

        Ok(self.shutdown(reason).await)
    }

    fn get_slow_fast_chains(config: &Config) -> (Chain, Chain) {
        unimplemented!()
    }

    fn get_pair(config: &Config, chain: &Chain) -> Pair {
        unimplemented!()
    }

    #[instrument(skip_all)]
    async fn shutdown(mut self, reason: eyre::Result<&'static str>) {
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

        // Shutdown strategy worker
        if let Err(e) = self.strategy_handle.shutdown().await {
            error!("Failed to shutdown strategy worker: {}", e);
        }
    }
}
