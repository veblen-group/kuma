use std::{collections::HashMap, sync::Arc, time::Duration};

use color_eyre::eyre::{self, Context, eyre};
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument, warn};

use crate::{execution, strategy};
use kuma_core::{
    chain::Chain,
    collector,
    config::{Config, StrategyConfig},
    database,
};

pub(super) struct Kuma {
    shutdown_token: CancellationToken,
    block_handles: HashMap<Chain, collector::Handle>,
    eth_handles: HashMap<Chain, collector::eth::Handle>,
    tycho_handles: HashMap<Chain, collector::tycho::Handle>,
    strategy_handles: Vec<strategy::Handle>,
    trade_execution_handle: execution::Handle,
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

        let mut block_handles = HashMap::new();
        let mut eth_handles = HashMap::new();
        let mut tycho_handles = HashMap::new();
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
                let (block_handle, eth_handle, tycho_handle) = collector::Builder {
                    chain: chain.clone(),
                    tycho_url: chain.tycho_url.clone(),
                    tycho_api_key: cfg.tycho_api_key.clone(),
                    token_addrs: addrs_for_chain[&chain].clone(),
                    add_tvl_threshold: cfg.add_tvl_threshold,
                    remove_tvl_threshold: cfg.remove_tvl_threshold,
                    shutdown_token: shutdown_token.clone(),
                }
                .build()
                .wrap_err("failed to start collectors for chain : {chain}")?;
                block_handles.entry(chain.clone()).or_insert(block_handle);
                eth_handles.entry(chain.clone()).or_insert(eth_handle);
                tycho_handles.entry(chain.clone()).or_insert(tycho_handle);
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

            let slow_usdc = todo!();
            let slow_stream = block_handles[&strategy.slow_chain]
                .get_block_state_stream(strategy.slow_pair.clone(), slow_usdc);

            let fast_usdc = todo!();
            let fast_stream = block_handles[&strategy.fast_chain]
                .get_block_state_stream(strategy.fast_pair.clone(), fast_usdc);

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

        // Create trade execution handle that subscribes to this strategy's signals
        let trade_execution_handle = execution::Builder {
            signal_rxs: strategy_handles
                .iter()
                .map(|handle| (handle.strategy_config(), handle.get_signal_rx()))
                .collect(),
            db: db.clone(),
        }
        .build()
        .wrap_err("failed to build trade execution worker")?;

        Ok(Self {
            shutdown_token,
            block_handles,
            eth_handles,
            tycho_handles,
            strategy_handles,
            trade_execution_handle,
        })
    }

    pub(super) async fn run(mut self) -> eyre::Result<()> {
        let block_futs = self
            .block_handles
            .iter_mut()
            .map(|(chain, handle)| {
                let chain = chain.clone();
                Box::pin(async move {
                    match handle.await {
                        Ok(()) => Ok(format!("{} block collector task completed", chain)),
                        Err(e) => Err(e),
                    }
                })
            })
            .collect::<Vec<_>>();

        let eth_futs = self
            .eth_handles
            .iter_mut()
            .map(|(chain, handle)| {
                let chain = chain.clone();
                Box::pin(async move {
                    match handle.await {
                        Ok(()) => Ok(format!("{} eth collector task completed", chain)),
                        Err(e) => Err(e),
                    }
                })
            })
            .collect::<Vec<_>>();

        let tycho_futs = self
            .tycho_handles
            .iter_mut()
            .map(|(chain, handle)| {
                let chain = chain.clone();
                Box::pin(async move {
                    match handle.await {
                        Ok(()) => Ok(format!("{} tycho collector task completed", chain)),
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

        let mut trade_execution_fut = {
            let handle = &mut self.trade_execution_handle;
            Box::pin(async move {
                match handle.await {
                    Ok(()) => Ok("trade execution task completed".to_owned()),
                    Err(e) => Err(e),
                }
            })
        };

        let reason: eyre::Result<String> = {
            select! {
            biased;

            () = self.shutdown_token.cancelled() => Ok("received shutdown signal".to_owned()),

            // Handle block collector task completion
            (result, _i, _block_collectors) = futures::future::select_all(block_futs) => {
                match result {
                    Ok(message) => Ok(message),
                    Err(e) => Err(e),
                }
            }

            // Handle eth collector task completion
            (result, _i, _eth_collectors) = futures::future::select_all(eth_futs) => {
                match result {
                    Ok(message) => Ok(message),
                    Err(e) => Err(e),
                }
            }

            // Handle tycho collector task completion
            (result, _i, _tycho_collectors) = futures::future::select_all(tycho_futs) => {
                match result {
                    Ok(message) => Ok(message),
                    Err(e) => Err(e),
                }
            }

            // Handle strategy worker task completion
            (result, _i, _strategies) = futures::future::select_all(strategy_futs) => {
                match result {
                    Ok(message) => Ok(message),
                    Err(e) => Err(e),
                }
            }

            // Handle trade execution task completion
            result = trade_execution_fut.as_mut() => {
                match result {
                    Ok(message) => Ok(message),
                    Err(e) => Err(e),
                }
            }
            }
        };

        self.shutdown(reason).await;
        Ok(())
    }

    #[instrument(skip_all)]
    async fn shutdown(mut self, reason: eyre::Result<String>) {
        const WAIT_BEFORE_ABORT: Duration = Duration::from_secs(25);

        // trigger the shutdown token in case it wasn't triggered yet
        self.shutdown_token.cancel();

        let message = format!(
            "waiting {} for all subtasks to shutdown before aborting",
            humantime::format_duration(WAIT_BEFORE_ABORT)
        );
        match &reason {
            Ok(reason) => info!(%reason, message),
            Err(reason) => error!(?reason, message),
        };

        // Shutdown strategy workers
        for mut handle in self.strategy_handles {
            if let Err(e) = handle.shutdown().await {
                error!("Failed to shutdown strategy worker: {}", e);
            }
        }

        // Shutdown block collector workers
        for (chain, mut handle) in self.block_handles {
            if let Err(e) = handle.shutdown().await {
                error!(
                    "Failed to shutdown block collector for {}: {}",
                    chain.name, e
                )
            }
        }

        // Shutdown trade execution worker
        if let Err(e) = self.trade_execution_handle.shutdown().await {
            error!("Failed to shutdown trade execution worker: {}", e);
        }
        // Shutdown eth collector workers
        for (chain, mut handle) in self.eth_handles {
            if let Err(e) = handle.shutdown().await {
                error!("Failed to shutdown eth collector for {}: {}", chain.name, e)
            }
        }

        // Shutdown tycho collector workers
        for (chain, mut handle) in self.tycho_handles {
            if let Err(e) = handle.shutdown().await {
                error!(
                    "Failed to shutdown tycho collector for {}: {}",
                    chain.name, e
                )
            }
        }
    }
}
