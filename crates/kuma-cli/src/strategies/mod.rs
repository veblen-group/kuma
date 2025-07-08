use crate::chain::ChainInfo;
use crate::strategies::utils::{get_amounts_limits, get_pool_limits};
use crate::tycho::state_update::AssetStateUpdate;
use color_eyre::eyre::{self, ContextCompat};
use serde::de;
use std::pin::Pin;
use std::time::Duration;
use tokio::{select, signal};
use tycho_execution::encoding::models::{Solution, Swap};

mod builder;
mod utils;
use futures::StreamExt;
use num_bigint::BigUint;
use tokio::time::sleep;
use tycho_simulation::models::Token;

#[derive(Debug, Clone)]
pub(crate) struct SwapInfo {
    pub(crate) given_token: Token,
    pub(crate) given_amount: BigUint, // Amount in
    pub(crate) checked_token: Token,
    pub(crate) checked_amount: BigUint,   // Minimum amount out
    pub(crate) split: Option<u32>,        // Percentage of the swap, if applicable
    pub(crate) user_data: Option<String>, // Additional user data, if needed
}

impl SwapInfo {
    pub(crate) fn new(
        swap_candidate: SwapCandidate,
        split: Option<u32>,
        user_data: Option<String>,
    ) -> Self {
        Self {
            given_token: swap_candidate.from,
            given_amount: swap_candidate.amount_in,
            checked_token: swap_candidate.to,
            checked_amount: swap_candidate.amount_out,
            split,
            user_data,
        }
    }
}
#[derive(Debug, Clone)]
pub(crate) struct Signal {
    pub(crate) component: String,
    pub(crate) slow_chain_swap: SwapInfo,
    pub(crate) fast_chain_swap: SwapInfo,
}

impl Signal {
    pub(crate) fn build(
        component: String,
        slow_chain_swap: SwapInfo,
        fast_chain_swap: SwapInfo,
    ) -> Self {
        Self {
            component,
            slow_chain_swap,
            fast_chain_swap,
        }
    }
}
#[derive(Debug, Clone)]
pub(crate) struct ArbParams {
    pub(crate) slippage_bps: u32,    // Slippage in basis points
    pub(crate) risk_factor_bps: u32, // Risk factor in basis points
}
#[derive(Debug, Clone)]
pub(crate) struct SwapCandidate {
    pub(crate) from: Token,
    pub(crate) to: Token,
    pub(crate) amount_in: BigUint,
    pub(crate) amount_out: BigUint,
}

#[derive(Debug, Clone)]
pub(crate) struct TradeAmounts {
    sell_a: BigUint,
    sell_b: BigUint,
}
struct SingleHopArbitrage {
    token_a: Token,
    token_b: Token,
    slow_chain_info: ChainInfo,
    fast_chain_info: ChainInfo,
    slow_chain_state: Pin<Box<dyn futures::Stream<Item = AssetStateUpdate> + Send>>,
    fast_chain_state: Pin<Box<dyn futures::Stream<Item = AssetStateUpdate> + Send>>,
    arb_params: ArbParams, // Arbitrage parameters, e.g., slippage, risk factor
}

impl SingleHopArbitrage {
    async fn run(&mut self) -> eyre::Result<()> {
        loop {
            select! {
                Some(slow_chain_asset_state) = self.slow_chain_state.next() => {
                    let fast_chain_state_stream = &mut self.fast_chain_state;
                    let arb_params = self.arb_params.clone();
                    let token_a = self.token_a.clone();
                    let token_b = self.token_b.clone();
                    let slow_chain_info = self.slow_chain_info.clone();
                    let fast_chain_info = self.fast_chain_info.clone();

                    if let Err(err) = handle_arbitrage(
                        slow_chain_asset_state,
                        fast_chain_state_stream,
                        arb_params,
                        token_a,
                        token_b,
                        slow_chain_info,
                        fast_chain_info,
                    ).await {
                        tracing::error!("Arbitrage handling failed: {:?}", err);
                    }
                }
            }
        }
    }
}

async fn handle_arbitrage(
    slow_chain_asset_state: AssetStateUpdate,
    fast_chain_state_stream: &mut Pin<Box<dyn futures::Stream<Item = AssetStateUpdate> + Send>>,
    arb_params: ArbParams,
    token_a: Token,
    token_b: Token,
    _slow_chain_info: ChainInfo,
    _fast_chain_info: ChainInfo,
) -> eyre::Result<()> {
    let inventory_amount = slow_chain_asset_state.inventory.clone();

    let swap_limits = get_amounts_limits(
        slow_chain_asset_state.state.clone(),
        &token_a,
        &token_b,
        &inventory_amount,
    )?;

    let a_to_b_swap_candidate = generate_swap_candidate(
        slow_chain_asset_state.state.clone(),
        token_a.clone(),
        token_b.clone(),
        swap_limits.sell_a.clone(),
        arb_params.slippage_bps,
        arb_params.risk_factor_bps,
    );

    let b_to_a_swap_candidate = generate_swap_candidate(
        slow_chain_asset_state.state.clone(),
        token_b.clone(),
        token_a.clone(),
        swap_limits.sell_b.clone(),
        arb_params.slippage_bps,
        arb_params.risk_factor_bps,
    );

    tracing::debug!(
        "Generated swap candidates: {:?} and {:?}",
        a_to_b_swap_candidate,
        b_to_a_swap_candidate
    );

    // Wait some time before fetching fast-chain state
    sleep(Duration::from_millis(200)).await;

    let fast_chain_asset_state = fast_chain_state_stream
        .next()
        .await
        .wrap_err("Failed to get fast chain asset state")?;

    let fast_swap_limits =
        get_pool_limits(fast_chain_asset_state.state.clone(), &token_a, &token_b)?;

    let reverse_arbs = evaluate_reverse_arbs(
        &fast_chain_asset_state.state,
        &[a_to_b_swap_candidate, b_to_a_swap_candidate],
        arb_params.slippage_bps,
        (
            (token_a.clone(), fast_swap_limits.sell_a.clone()),
            (token_b.clone(), fast_swap_limits.sell_b.clone()),
        ),
    )?;

    tracing::debug!("Evaluated reverse arbitrage candidates: {:?}", reverse_arbs);

    // TODO: Convert into executable Solution and forward to executor
    for (candidate, reverse_candidate, amount_back) in reverse_arbs {
        tracing::info!(
            "Profitable arb candidate: {:?} and reverse candidate {:?} with return: {}",
            candidate,
            reverse_candidate,
            amount_back
        );
        // TODO: pick the best candidate based on expected profit. move outside loop
        let slow_chain_swap = SwapInfo::new(candidate.clone(), None, None);
        let fast_chain_swap = SwapInfo::new(reverse_candidate.clone(), None, None);

        // TODO: send signal to executer
        let _signal = Signal::build(
            "SingleHopArbitrage".to_string(), // what is the component?
            slow_chain_swap,
            fast_chain_swap,
        );
    }

    Ok(())
}

fn generate_swap_candidate(
    state: Box<dyn tycho_simulation::protocol::state::ProtocolSim>,
    from: Token,
    to: Token,
    amount_in: BigUint,
    slipage_bps: u32,
    risk_factor_bps: u32,
) -> SwapCandidate {
    let amount_out = state
        .get_amount_out(amount_in.clone(), &from, &to)
        .expect("Failed to get amount out")
        .amount;
    let minimum_amout_out = calculate_minimum_amount_out(&amount_out, slipage_bps, risk_factor_bps);
    SwapCandidate {
        from,
        to,
        amount_in,
        amount_out: minimum_amout_out,
    }
}

fn calculate_minimum_amount_out(
    amount_out: &BigUint,
    slippage_bps: u32,
    risk_factor_bps: u32,
) -> BigUint {
    let slippage = amount_out * BigUint::from(slippage_bps) / BigUint::from(10_000u32);
    let risk_factor = amount_out * BigUint::from(risk_factor_bps) / BigUint::from(10_000u32);
    amount_out - slippage - risk_factor
}

fn evaluate_reverse_arbs(
    state: &Box<dyn tycho_simulation::protocol::state::ProtocolSim>,
    candidates: &[SwapCandidate],
    slippage_bps: u32,
    swap_limit: ((Token, BigUint), (Token, BigUint)),
) -> eyre::Result<Vec<(SwapCandidate, SwapCandidate, BigUint)>> {
    let mut results = vec![];

    for cand in candidates {
        // B → A direction
        let amount_out = state.get_amount_out(cand.amount_out.clone(), &cand.to, &cand.from)?;
        let slippage = &amount_out.amount * BigUint::from(slippage_bps) / BigUint::from(10_000u32);
        let amount_out_adjusted = amount_out.amount - slippage;
        if &amount_out_adjusted > &cand.amount_in {
            let reverse_candidate = SwapCandidate {
                from: cand.to.clone(),
                to: cand.from.clone(),
                amount_in: cand.amount_out.clone(),
                amount_out: amount_out_adjusted.clone(),
            };
            results.push((cand.clone(), reverse_candidate, amount_out_adjusted));
        }
    }

    Ok(results)
}

#[test]
fn test_calculate_minimum_amount_out() {
    let amount_out = BigUint::from(10000u32);
    let slippage_bps = 100; // 1%
    let risk_bps = 50; // 0.5%
    let min_out = calculate_minimum_amount_out(&amount_out, slippage_bps, risk_bps);

    // 10000 - 1% - 0.5% = 9850
    assert_eq!(min_out, BigUint::from(9850u32));
}
