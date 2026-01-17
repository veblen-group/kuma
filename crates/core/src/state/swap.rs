use crate::{
    state::{erc20::Transfer, pair::Pair},
    strategy,
};
use alloy::{primitives::Address, rpc::types::TransactionReceipt};
use color_eyre::eyre::{self};
use num_bigint::BigUint;
use num_traits::CheckedAdd as _;
use serde::{Deserialize, Serialize};
use tycho_simulation::tycho_common::models::token::Token;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Swap {
    pub token_in: Token,
    pub amount_in: BigUint,
    pub token_out: Token,
    pub amount_out: BigUint,
    #[allow(dead_code)]
    pub gas_cost: u64,
}

impl Swap {
    pub fn try_from_receipts(
        receipt: &TransactionReceipt, // should be trade log
        swap: strategy::Swap,
    ) -> eyre::Result<Self> {
        let mut amount_in = BigUint::default();
        let mut amount_out = BigUint::default();

        let token_in_addr = Address::from_slice(&swap.token_in.address);
        let token_out_addr = Address::from_slice(&swap.token_out.address);

        for log in receipt.logs() {
            let contract = log.address();

            if contract == token_in_addr {
                let transfer = Transfer::try_from_log(&swap.token_in, log.clone())?;
                amount_in = amount_in
                    .checked_add(&transfer.amount)
                    .ok_or_else(|| eyre::eyre!("overflow adding amount_in"))?;
            } else if contract == token_out_addr {
                let transfer = Transfer::try_from_log(&swap.token_out, log.clone())?;
                amount_out = amount_out
                    .checked_add(&transfer.amount)
                    .ok_or_else(|| eyre::eyre!("overflow adding amount_out"))?;
            }
        }

        Ok(Swap {
            token_in: swap.token_in,
            amount_in: amount_in,
            token_out: swap.token_out,
            amount_out: amount_out,
            gas_cost: receipt.gas_used * receipt.effective_gas_price as u64,
        })
    }

    pub fn get_pair(&self) -> Pair {
        Pair::new(self.token_in.clone(), self.token_out.clone())
    }
}
