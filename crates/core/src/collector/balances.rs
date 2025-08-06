use std::{
    pin::Pin,
    task::{Context, Poll},
};

use alloy::{
    eips::BlockNumberOrTag,
    primitives::{Address, U256},
    providers::Provider,
    pubsub::SubscriptionStream,
    rpc::types::{Filter, Log},
    sol,
    sol_types::SolEvent,
};
use color_eyre::eyre;
use futures::{Stream, StreamExt as _};
use num_bigint::BigUint;

// Taken from https://github.com/OpenZeppelin/openzeppelin-contracts/blob/3790c59623e99cb0272ddf84e6a17a5979d06b35/contracts/token/ERC20/IERC20.sol
sol!(
    #[sol(rpc)]
    contract IERC20 {
        function balanceOf(address account) external view returns (uint256);
        event Transfer(address indexed from, address indexed to, uint256 value);
    }
);

/// A stream of an account’s ERC-20 balance over time.
pub struct BalanceStream {
    account_addr: Address,
    current_balance: BigUint,
    logs: Pin<Box<SubscriptionStream<Log>>>,
}

impl BalanceStream {
    /// Connect provider, set up client, fetch start balance, subscribe to logs.
    pub async fn init<P: Provider + Clone>(
        provider: P,
        token_addr: Address,
        account_addr: Address,
    ) -> eyre::Result<Self> {
        // get token contract handle
        let token = IERC20::new(token_addr, provider.clone());

        // get initial balance
        let start: U256 = token.balanceOf(account_addr).call().await?;
        let current_balance = BigUint::from_bytes_be(&start.to_be_bytes::<32usize>());

        // set up log stream
        let filter = Filter::new()
            .address(token_addr)
            .event(IERC20::Transfer::SIGNATURE)
            .from_block(BlockNumberOrTag::Latest);
        let sub = provider.subscribe_logs(&filter).await?;
        let event_stream = Box::pin(sub.into_stream());

        Ok(Self {
            account_addr,
            current_balance,
            logs: event_stream,
        })
    }
}

impl Stream for BalanceStream {
    type Item = eyre::Result<BigUint>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        match this.logs.poll_next_unpin(cx) {
            Poll::Ready(Some(log)) => {
                // Decode the Transfer event
                let event = log.log_decode::<IERC20::Transfer>()?;
                let IERC20::Transfer { from, to, value } = event.inner.data;

                // Convert U256 to BigUint
                let value = BigUint::from_bytes_be(&value.to_be_bytes::<32usize>());

                // Update balance if our account is involved
                if from == this.account_addr {
                    this.current_balance -= &value;
                }
                if to == this.account_addr {
                    this.current_balance += &value;
                }

                Poll::Ready(Some(Ok(this.current_balance.clone())))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
