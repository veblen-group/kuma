use core::{
    config::{Config, StrategyConfig},
    signals,
};

use crate::{StrategyArgs, kuma::Kuma};
use color_eyre::eyre::{self, Ok, eyre};
use tokio_util::sync::CancellationToken;
use tracing::info;

pub(crate) async fn run(
    args: StrategyArgs,
    config: Config,
    shutdown_token: CancellationToken,
) -> eyre::Result<signals::CrossChainSingleHop> {
    let strategy = StrategyConfig {
        token_a: self.args.token_a.clone(),
        token_b: self.args.token_b.clone(),
        slow_chain: self.args.slow_chain.clone(),
        fast_chain: self.args.fast_chain.clone(),
    };

    let kuma = Kuma::spawn(config, strategy, shutdown_token)
        .map_err(|e| eyre!("Failed to spawn Kuma: {e:}"))?;
    let signal = kuma
        .generate_signal()
        .await
        .map_err(|e| eyre!("Failed to generate signal: {e}"))?;

    info!(signal = ?signal, "Generated signal successfully");
    Ok(signal)
}
