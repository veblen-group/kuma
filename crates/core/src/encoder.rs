use std::str::FromStr as _;
use std::u64;

use alloy::consensus::EthereumTxEnvelope;
use alloy::network::EthereumWallet;
use alloy::primitives::{Address as alloyAddress, Bytes as AlloyBytes, Keccak256};
use alloy::providers::ext::AnvilApi;
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy::signers::Signature;
use alloy::signers::{SignerSync, local::PrivateKeySigner};
use alloy::sol_types::{SolStruct, SolValue, eip712_domain};
use color_eyre::eyre::{self, Context as _};
use num_bigint::BigUint;
use tracing::trace;
use tycho_common::{
    Bytes,
    models::{Address, token::Token},
};

use tycho_execution::encoding::errors::EncodingError;
use tycho_execution::encoding::evm::approvals::permit2::PermitSingle;
use tycho_execution::encoding::evm::encoder_builders::TychoRouterEncoderBuilder;
use tycho_execution::encoding::evm::utils::biguint_to_u256;
use tycho_execution::encoding::models::{
    self, EncodedSolution, Solution, Swap, Transaction, UserTransferType,
};
use tycho_simulation::protocol::models::ProtocolComponent;

use crate::chain::Chain;
use crate::signals::CrossChainSingleHop;
pub async fn get_tx_request(
    transaction: &Transaction,
    signer: &PrivateKeySigner,
    _chain: &alloy::core::primitives::ChainId,
    rpc_url: &str,
) -> eyre::Result<EthereumTxEnvelope<alloy::consensus::TxEip4844Variant>> {
    let wallet = EthereumWallet::new(signer.clone());
    let provider = alloy::providers::ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(rpc_url.parse().wrap_err("Invalid RPC URL")?);

    provider.anvil_set_logging(true).await.ok();
    let base_request = TransactionRequest::default()
        .to(alloyAddress::from_slice(&transaction.to))
        .input(TransactionInput {
            input: Some(AlloyBytes::from(transaction.data.clone())),
            data: None,
        })
        .value(biguint_to_u256(&transaction.value));

    let tx = provider
        .fill(base_request.clone())
        .await
        .wrap_err("failed filling tx")?
        .try_into_envelope()?;
    Ok(tx)
}

pub fn try_transactions_from_signal(
    signal: CrossChainSingleHop,
    slow_signer: PrivateKeySigner,
    fast_signer: PrivateKeySigner,
) -> eyre::Result<(Transaction, Transaction)> {
    let slow_signer_address = Address::from_str(&slow_signer.address().to_string())
        .wrap_err("Failed to parse signer address")?;
    let fast_signer_address = Address::from_str(&fast_signer.address().to_string())
        .wrap_err("Failed to parse signer address")?;

    let (slow_solution, fast_solution) =
        try_solutions_from_signal(signal.clone(), slow_signer_address, fast_signer_address)?;
    let slow_chain = signal.slow_chain.clone();
    let fast_chain = signal.fast_chain.clone();

    let encoded_slow_solutions = encode_solution(slow_solution.clone(), &slow_chain.clone())?;
    let encoded_fast_solution = encode_solution(fast_solution.clone(), &fast_chain.clone())?;

    let slow_native_address = Bytes::from(signal.slow_chain.name.native_token().address.as_ref());
    let fast_native_address = Bytes::from(signal.fast_chain.name.native_token().address.as_ref());

    let slow_tx = encode_tycho_router_call(
        signal.slow_chain.chain_id(),
        encoded_slow_solutions,
        &slow_solution,
        slow_native_address,
        slow_signer.clone(),
    )?;
    let fast_tx = encode_tycho_router_call(
        signal.fast_chain.chain_id(),
        encoded_fast_solution,
        &fast_solution,
        fast_native_address,
        fast_signer.clone(),
    )?;

    Ok((slow_tx, fast_tx))
}

fn encode_tycho_router_call(
    chain_id: u64,
    encoded_solution: EncodedSolution,
    solution: &Solution,
    native_address: Bytes,
    signer: PrivateKeySigner,
) -> Result<Transaction, EncodingError> {
    let p = encoded_solution.permit.expect("Permit object must be set");
    let permit = PermitSingle::try_from(&p)
        .map_err(|_| EncodingError::InvalidInput("Invalid permit".to_string()))?;
    trace!("Signing permit2 approval: {:?}", permit);
    let signature = sign_permit(chain_id, &p, signer)?;
    let given_amount = biguint_to_u256(&solution.given_amount);
    let min_amount_out = biguint_to_u256(&solution.checked_amount);
    let given_token = alloyAddress::from_slice(&solution.given_token);
    let checked_token = alloyAddress::from_slice(&solution.checked_token);
    let receiver = alloyAddress::from_slice(&solution.receiver);

    let method_calldata = (
        given_amount,
        given_token,
        checked_token,
        min_amount_out,
        false,
        false,
        receiver,
        permit,
        signature.as_bytes().to_vec(),
        encoded_solution.swaps,
    )
        .abi_encode();
    let contract_interaction = encode_input(&encoded_solution.function_signature, method_calldata);
    let value = if solution.given_token == native_address {
        solution.given_amount.clone()
    } else {
        BigUint::ZERO
    };
    Ok(Transaction {
        to: encoded_solution.interacting_with,
        value,
        data: contract_interaction,
    })
}

fn sign_permit(
    chain_id: u64,
    permit_single: &models::PermitSingle,
    signer: PrivateKeySigner,
) -> Result<Signature, EncodingError> {
    // TODO: make permit2 address configurable
    let permit2_address = alloyAddress::from_str("0x000000000022D473030F116dDEE9F6B43aC78BA3")
        .map_err(|_| EncodingError::FatalError("Permit2 address not valid".to_string()))?;
    let domain = eip712_domain! {
        name: "Permit2",
        chain_id: chain_id,
        verifying_contract: permit2_address,
    };
    let permit_single: PermitSingle = PermitSingle::try_from(permit_single)?;
    let hash = permit_single.eip712_signing_hash(&domain);
    signer.sign_hash_sync(&hash).map_err(|e| {
        EncodingError::FatalError(format!("Failed to sign permit2 approval with error: {e}"))
    })
}

fn try_solutions_from_signal(
    signal: CrossChainSingleHop,
    slow_user_address: Bytes,
    fast_user_address: Bytes,
) -> eyre::Result<(Solution, Solution)> {
    let sell_token_slow = signal.slow_swap_sim.token_in;
    let buy_token_slow = signal.slow_swap_sim.token_out;

    let sell_token_fast = signal.fast_swap_sim.token_in;
    let buy_token_fast = signal.fast_swap_sim.token_out;

    let slow_chain_amount_in = signal.slow_swap_sim.amount_in;
    let slow_chain_min_amount_out = signal.slow_swap_sim.amount_out;

    let slow_protocol_component = signal.slow_protocol_component.unwrap().as_ref().clone();
    let fast_chain_amount_in = signal.fast_swap_sim.amount_in;
    let fast_chain_min_amount_out = signal.fast_swap_sim.amount_out;

    let fast_protocol_component = signal.fast_protocol_component.unwrap().as_ref().clone();

    let slow_solution = create_solution(
        slow_protocol_component,
        &sell_token_slow,
        &buy_token_slow,
        slow_chain_amount_in,
        slow_chain_min_amount_out,
        slow_user_address.clone(),
    );

    let fast_solution = create_solution(
        fast_protocol_component,
        &sell_token_fast,
        &buy_token_fast,
        fast_chain_amount_in,
        fast_chain_min_amount_out,
        fast_user_address.clone(),
    );

    Ok((slow_solution, fast_solution))
}

fn create_solution(
    component: ProtocolComponent,
    sell_token: &Token,
    buy_token: &Token,
    amount_in: BigUint,
    min_amount_out: BigUint,
    user_address: Bytes,
) -> Solution {
    // Convert tycho_simulation bytes to tycho_common bytes by converting through hex string
    let sell_address = Bytes::from(sell_token.address.0.clone());
    let buy_address = Bytes::from(buy_token.address.0.clone());
    let simple_swap = Swap::new(
        component,
        sell_address.clone(),
        buy_address.clone(),
        // Split defines the fraction of the amount to be swapped. A value of 0 indicates 100% of
        // the amount or the total remaining balance.
        0f64,
        None,
        None, // protocol_sim
        None, // output_amount
    );

    Solution {
        sender: user_address.clone(),
        receiver: user_address,
        given_token: sell_address,
        given_amount: amount_in,
        checked_token: buy_address,
        exact_out: false, // it's an exact in solution
        checked_amount: min_amount_out,
        swaps: vec![simple_swap],
        native_action: None,
    }
}

/// Encodes the input data for a function call to the given function selector.
pub fn encode_input(selector: &str, mut encoded_args: Vec<u8>) -> Vec<u8> {
    let mut hasher = Keccak256::new();
    hasher.update(selector.as_bytes());
    let selector_bytes = &hasher.finalize()[..4];
    let mut call_data = selector_bytes.to_vec();
    // Remove extra prefix if present (32 bytes for dynamic data)
    // Alloy encoding is including a prefix for dynamic data indicating the offset or length
    // but at this point we don't want that
    if encoded_args.len() > 32
        && encoded_args[..32]
            == [0u8; 31]
                .into_iter()
                .chain([32].to_vec())
                .collect::<Vec<u8>>()
    {
        encoded_args = encoded_args[32..].to_vec();
    }
    call_data.extend(encoded_args);
    call_data
}

pub(crate) fn encode_solution(solution: Solution, chain: &Chain) -> eyre::Result<EncodedSolution> {
    // Set RPC_URL environment variable if not already set
    if std::env::var("RPC_URL").is_err() {
        unsafe { std::env::set_var("RPC_URL", &chain.rpc_url) };
    }

    // Initialize the encoder
    let encoder = TychoRouterEncoderBuilder::new()
        .chain(chain.name) // Default for now
        .user_transfer_type(UserTransferType::TransferFromPermit2)
        .build()
        .expect("Failed to build encoder");

    // Encode the solution
    let encoded_solution = encoder
        .encode_solutions(vec![solution.clone()])
        .expect("Failed to encode router calldata")[0]
        .clone();

    trace!("Encoded solution: {:?}", encoded_solution);

    Ok(encoded_solution)
}
