use std::{collections::HashMap, str::FromStr as _, time::Duration};

use color_eyre::eyre::{self, Context};
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument};

use crate::{config::Config, strategy};
use kuma_core::{chain::Chain, collector, config::StrategyConfig, state::pair::Pair};

pub(super) struct Kuma {
    shutdown_token: CancellationToken,
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

            //  get the pairs for the chains from strategy config
            let chain_pairs =
                kuma_core::config::Config::get_chain_pairs(&token_a, &token_b, &inventory);
            //  initialize pair and chain info
            let (slow_chain, fast_chain) = (
                chain_pairs
                    .keys()
                    .find(|chain| {
                        chain.name
                            == tycho_common::models::Chain::from_str(&slow_chain)
                                .expect("invalid slow chain name: {slow_chain}")
                    })
                    .expect("invalid slow chain name"),
                chain_pairs
                    .keys()
                    .find(|chain| {
                        chain.name
                            == tycho_common::models::Chain::from_str(&fast_chain)
                                .expect("invalid fast chain name: {fast_chain}")
                    })
                    .expect("invalid fast chain name"),
            );
            let (slow_pair, fast_pair) = (&chain_pairs[&slow_chain], &chain_pairs[&fast_chain]);

            // get inventory
            let slow_inventory = (
                inventory[slow_chain][slow_pair.token_a()].clone(),
                inventory[slow_chain][slow_pair.token_b()].clone(),
            );
            let fast_inventory = (
                inventory[fast_chain][fast_pair.token_a()].clone(),
                inventory[fast_chain][fast_pair.token_b()].clone(),
            );

            // get streams from collector handles
            let slow_stream = collector_handles[slow_chain].get_pair_state_stream(&slow_pair);
            let fast_stream = collector_handles[fast_chain].get_pair_state_stream(&fast_pair);

            let strategy = kuma_core::strategy::CrossChainSingleHop {
                slow_pair: slow_pair.clone(),
                slow_chain: slow_chain.clone(),
                fast_pair: fast_pair.clone(),
                fast_chain: fast_chain.clone(),
                slow_inventory,
                fast_inventory,
                binary_search_steps: cfg.binary_search_steps,
                max_slippage_bps: cfg.max_slippage_bps,
                congestion_risk_discount_bps: cfg.congestion_risk_discount_bps,
            };
            let strategy_handle = todo!("build the strategy worker handle");
            strategy_handle
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
