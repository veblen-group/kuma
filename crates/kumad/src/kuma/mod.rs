use std::{collections::HashMap, time::Duration};

use color_eyre::eyre::{self, Context};
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument, warn};

use crate::{config::Config, strategy};
use kuma_core::{chain::Chain, collector, config::StrategyConfig};

pub(super) struct Kuma {
    shutdown_token: CancellationToken,
    #[allow(dead_code)]
    collector_handles: HashMap<Chain, collector::Handle>,
    strategy_handle: strategy::Handle,
}

impl Kuma {
    pub(super) fn new(cfg: Config, shutdown_token: CancellationToken) -> eyre::Result<Self> {
        let cfg = cfg.core;

        // 1. extract from config, for each chain:
        //  1. token addrs
        //  2. inventory
        let (addrs_for_chain, inventory) = cfg
            .build_addrs_and_inventory()
            .wrap_err("failed to parse chain assets")?;

        info!("Parsed {} chains from config:", addrs_for_chain.len());
        for (chain, tokens) in &addrs_for_chain {
            info!(name = %chain.name,
                        chain_id = %chain.metadata.id(),
                        token_count = %tokens.len(),
                        "🔗 Initialized chain info from config")
        }

        // 2. set up collectors for each chain
        let collector_handles: HashMap<Chain, collector::Handle> = addrs_for_chain
            .into_iter()
            .map(|(chain, addrs)| {
                let handle = collector::Builder {
                    chain: chain.clone(),
                    tycho_url: chain.tycho_url.clone(),
                    api_key: cfg.tycho_api_key.clone(),
                    tokens: addrs,
                    add_tvl_threshold: cfg.add_tvl_threshold,
                    remove_tvl_threshold: cfg.remove_tvl_threshold,
                }
                .build()
                .wrap_err("failed to start tycho collector for chain : {chain}")?;
                Ok((chain.clone(), handle))
            })
            .collect::<eyre::Result<HashMap<Chain, collector::Handle>>>()?;

        // TODO: this should run for each strategy config
        let strategy_handle = {
            let StrategyConfig {
                token_a,
                token_b,
                slow_chain,
                fast_chain,
            } = &cfg.strategies[0];

            let strategy = kuma_core::strategy::Builder {
                token_a: token_a.clone(),
                token_b: token_b.clone(),
                slow_chain_name: slow_chain.clone(),
                fast_chain_name: fast_chain.clone(),
                inventory,
                binary_search_steps: cfg.binary_search_steps,
                max_slippage_bps: cfg.max_slippage_bps,
                congestion_risk_discount_bps: cfg.congestion_risk_discount_bps,
            }
            .build()
            .wrap_err("failed to build strategy")?;

            let slow_stream =
                collector_handles[&strategy.slow_chain].get_pair_state_stream(&strategy.slow_pair);
            let fast_stream =
                collector_handles[&strategy.fast_chain].get_pair_state_stream(&strategy.fast_pair);

            let slow_block_time = strategy.slow_chain.metadata.average_blocktime_hint().unwrap_or_else(|| {
                warn!(chain = %strategy.slow_chain, "average block time metadata is missing for chain. defaulting to 12s");
                Duration::from_secs(12)
            });

            strategy::Builder {
                strategy,
                slow_stream,
                fast_stream,
                slow_block_time,
            }
            .build()
            .wrap_err("failed to build strategy worker")?
        };

        Ok(Self {
            shutdown_token,
            collector_handles,
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
