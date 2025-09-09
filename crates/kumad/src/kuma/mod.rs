use std::{collections::HashMap, sync::Arc, time::Duration};

use color_eyre::eyre::{self, Context, eyre};
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument, warn};

use crate::strategy;
use kuma_core::{
    chain::Chain,
    collector,
    config::{Config, StrategyConfig},
    database,
};

pub(super) struct Kuma {
    shutdown_token: CancellationToken,
    #[allow(dead_code)]
    collector_handles: HashMap<Chain, collector::Handle>,
    strategy_handles: Vec<strategy::Handle>,
}

impl Kuma {
    #[instrument(skip_all)]
    pub(super) fn new(cfg: Config, shutdown_token: CancellationToken) -> eyre::Result<Self> {
        // extract from config, for each chain:
        //  1. token addrs
        //  2. inventory
        let (addrs_for_chain, inventory) = cfg
            .build_addrs_and_inventory()
            .map_err(|e| eyre!("failed to parse chain assets: {}", e))?;

        info!("Parsed {} chains from config:", addrs_for_chain.len());
        for (chain, tokens) in &addrs_for_chain {
            info!(name = %chain.name,
                        chain_id = %chain.metadata.id(),
                        token_count = %tokens.len(),
                        "🔗 Initialized chain info from config")
        }

        let db = database::Handle::from_config(cfg.database, Arc::new(addrs_for_chain.clone()))?;

        let mut collector_handles = HashMap::new();
        let mut strategy_handles = vec![];

        for StrategyConfig {
            token_a,
            token_b,
            slow_chain,
            fast_chain,
        } in &cfg.strategies
        {
            let slow_chain = Config::get_chain_from_name(&slow_chain, addrs_for_chain.keys())?;
            let fast_chain = Config::get_chain_from_name(&fast_chain, addrs_for_chain.keys())?;

            // set up collectors for each chain
            for chain in [&slow_chain, &fast_chain] {
                collector_handles.entry(chain.clone()).or_insert(
                    collector::Builder {
                        chain: chain.clone(),
                        tycho_url: chain.tycho_url.clone(),
                        api_key: cfg.tycho_api_key.clone(),
                        token_addrs: addrs_for_chain[&chain].clone(),
                        add_tvl_threshold: cfg.add_tvl_threshold,
                        remove_tvl_threshold: cfg.remove_tvl_threshold,
                        shutdown_token: shutdown_token.clone(),
                    }
                    .build()
                    .wrap_err("failed to start tycho collector for chain : {chain}")?,
                );
            }

            let strategy = kuma_core::strategy::Builder {
                token_a: token_a.clone(),
                token_b: token_b.clone(),
                slow_chain: slow_chain.clone(),
                fast_chain: fast_chain.clone(),
                inventory: inventory.clone(),
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

            let slow_block_time = strategy
                .slow_chain
                .metadata
                .average_blocktime_hint()
                .expect("chain metadata for average block time not found");

            let strategy_handle = strategy::Builder {
                strategy,
                slow_stream,
                fast_stream,
                slow_block_time,
                db: db.clone(),
            }
            .build()
            .wrap_err("failed to build strategy worker")?;

            strategy_handles.push(strategy_handle);
        }

        Ok(Self {
            shutdown_token,
            collector_handles,
            strategy_handles,
        })
    }

    pub(super) async fn run(mut self) -> eyre::Result<()> {
        // TODO: maybe make a handle trait with Handle::shutdown() and Handle::id() and store all the impl Handles in a vec?
        let collector_futs = self
            .collector_handles
            .iter_mut()
            .map(|(chain, handle)| {
                let chain = chain.clone();
                Box::pin(async move {
                    match handle.await {
                        Ok(()) => Ok(format!("{} collector task completed", chain)),
                        Err(e) => Err(e),
                    }
                })
            })
            .collect::<Vec<_>>();

        let strategy_futs = self.strategy_handles.iter_mut().map(|handle| {
            Box::pin(async move {
                match handle.await {
                    Ok(()) => Ok("strategy task completed".to_owned()),
                    Err(e) => Err(e),
                }
            })
        });

        let reason: eyre::Result<String> = {
            loop {
                select! {
                    biased;

                    () = self.shutdown_token.cancelled() => break Ok("received shutdown signal".to_owned()),

                    // Handle collector task completion
                    (result, _i, _collectors) = futures::future::select_all(collector_futs) => {
                        match result {
                            Ok(message) => break Ok(message),
                            Err(e) => break Err(e),
                        }
                    }

                    // Handle strategy worker task completion
                    (result, _i, _strategies) = futures::future::select_all(strategy_futs) => {
                        match result {
                            Ok(message) => break Ok(message),
                            Err(e) => break Err(e),
                        }
                    }
                }
            }
        };

        Ok(self.shutdown(reason).await)
    }

    #[instrument(skip_all)]
    async fn shutdown(self, reason: eyre::Result<String>) {
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

        // Shutdown strategy workers
        for mut handle in self.strategy_handles {
            if let Err(e) = handle.shutdown().await {
                error!("Failed to shutdown strategy worker: {}", e);
            }
        }

        // Shutdown collector workers
        for (chain, mut handle) in self.collector_handles {
            if let Err(e) = handle.shutdown().await {
                error!("Failed to shutdown collector for {}: {}", chain.name, e)
            }
        }
    }
}
