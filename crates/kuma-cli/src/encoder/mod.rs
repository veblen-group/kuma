use std::str::FromStr as _;
use std::{collections::HashMap, hash::Hash};

use alloy::eips::BlockNumberOrTag;
use alloy::network::{Ethereum, EthereumWallet};
use alloy::primitives::{Address, Bytes as AlloyBytes, Keccak256, TxKind, U256};
use alloy::providers::fillers::{FillProvider, JoinFill, WalletFiller};
use alloy::providers::{Identity, Provider, RootProvider};
use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy::signers::{Signature, Signer};
use alloy::signers::{SignerSync, local::PrivateKeySigner};
use alloy::sol_types::{SolStruct, SolValue, eip712_domain};
use color_eyre::eyre::{self, Context as _};
use num_bigint::BigUint;
use tycho_common::models::protocol::ProtocolComponent;
use tycho_common::{
    Bytes,
    models::{Chain, token::Token},
};

use tycho_execution::encoding::errors::EncodingError;
use tycho_execution::encoding::evm::approvals::permit2::PermitSingle;
use tycho_execution::encoding::evm::encoder_builders::TychoRouterEncoderBuilder;
use tycho_execution::encoding::evm::utils::biguint_to_u256;
use tycho_execution::encoding::models::{
    self, EncodedSolution, Solution, Swap, Transaction, UserTransferType,
};

use crate::signals::CrossChainSingleHop;

// TODO: sign and submit permit2 approval per new token once before sending any transactions.
//.      Allows sending transactions without the need for a seperate approval.
const PERMIT2_ADDRESS: &str = "0x000000000022D473030F116dDEE9F6B43aC78BA3"; // todo: check if this is the correct address
const USER_ADDRESS: &str = "0x0000000000000000000000000000000000000000"; // Placeholder, replace with actual user address

pub(crate) async fn generate_transactions_from_signal(
    signal: CrossChainSingleHop,
    slow_chain_provider: FillProvider<
        JoinFill<Identity, WalletFiller<EthereumWallet>>,
        RootProvider,
    >,
    fast_chain_provider: FillProvider<
        JoinFill<Identity, WalletFiller<EthereumWallet>>,
        RootProvider,
    >,
    private_key: &str,
) -> eyre::Result<(TransactionRequest, TransactionRequest)> {
    let slow_tx = build_slow_chain_transaction(&signal, private_key)?;
    let fast_tx = build_fast_chain_transaction(&signal, private_key)?;
    let slow_tx_request = get_swap_tx_request(
        slow_chain_provider,
        Address::from_str(USER_ADDRESS).expect("Invalid user address"),
        signal.slow_chain.chain_id(),
        slow_tx,
    )
    .await
    .wrap_err("Failed to build slow chain tx request")?;

    let fast_tx_request = get_swap_tx_request(
        fast_chain_provider,
        Address::from_str(USER_ADDRESS).expect("Invalid user address"),
        signal.fast_chain.chain_id(),
        fast_tx,
    )
    .await
    .wrap_err("Failed to build fast chain tx request")?;

    Ok((slow_tx_request, fast_tx_request))
}

async fn get_permit2_approve_tx_request(
    provider: FillProvider<
        JoinFill<Identity, WalletFiller<EthereumWallet>>,
        RootProvider<Ethereum>,
    >,
    user_address: Address,
    token_address: Address,
    chain_id: u64,
) -> TransactionRequest {
    let block = provider
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await
        .expect("Failed to fetch latest block")
        .expect("Block not found");

    let base_fee = block
        .header
        .base_fee_per_gas
        .expect("Base fee not available");
    let max_priority_fee_per_gas = 1_000_000_000u64;
    let max_fee_per_gas = base_fee + max_priority_fee_per_gas;

    let approve_function_signature = "approve(address,uint256)";
    let args = (
        Address::from_str(PERMIT2_ADDRESS).expect("Couldn't convert to address"),
        U256::MAX, // Approve maximum amount
    );
    let data = encode_input(approve_function_signature, args.abi_encode());
    let nonce = provider
        .get_transaction_count(user_address)
        .await
        .expect("Failed to get nonce");

    let approval_request = TransactionRequest {
        to: Some(TxKind::Call(token_address)),
        from: Some(user_address),
        value: None,
        input: TransactionInput {
            input: Some(AlloyBytes::from(data)),
            data: None,
        },
        gas: Some(100_000u64),
        chain_id: Some(chain_id),
        max_fee_per_gas: Some(max_fee_per_gas.into()),
        max_priority_fee_per_gas: Some(max_priority_fee_per_gas.into()),
        nonce: Some(nonce),
        ..Default::default()
    };
    approval_request
}

pub(crate) async fn get_swap_tx_request(
    provider: FillProvider<
        JoinFill<Identity, WalletFiller<EthereumWallet>>,
        RootProvider<Ethereum>,
    >,
    user_address: Address,
    chain_id: u64,
    tx: Transaction,
) -> eyre::Result<TransactionRequest> {
    let block = provider
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await
        .wrap_err("Failed to fetch latest block")?
        .ok_or_else(|| eyre::eyre!("Block not found"))?;

    let base_fee = block
        .header
        .base_fee_per_gas
        .ok_or_else(|| eyre::eyre!("Base fee not available"))?;
    let max_priority_fee_per_gas = 1_000_000_000u64;
    let max_fee_per_gas = base_fee + max_priority_fee_per_gas;

    let nonce = provider
        .get_transaction_count(user_address)
        .await
        .wrap_err("Failed to get nonce")?;

    let tx_request = TransactionRequest {
        to: Some(TxKind::Call(Address::from_slice(&tx.to))),
        from: Some(user_address),
        value: Some(biguint_to_u256(&tx.value)),
        input: TransactionInput {
            input: Some(AlloyBytes::from(tx.data)),
            data: None,
        },
        gas: Some(800_000u64), // Adjust gas limit as needed
        chain_id: Some(chain_id),
        max_fee_per_gas: Some(max_fee_per_gas.into()),
        max_priority_fee_per_gas: Some(max_priority_fee_per_gas.into()),
        nonce: Some(nonce),
        ..Default::default()
    };
    Ok(tx_request)
}

pub(crate) fn build_slow_chain_transaction(
    arb_signal: &CrossChainSingleHop,
    private_key: &str,
) -> eyre::Result<Transaction> {
    let slow_solution = create_slow_solution(arb_signal)?.clone();
    let chain = &arb_signal.slow_chain;
    let native_address = chain.name.native_token().address.clone();
    let signer = PrivateKeySigner::from_str(&private_key)
        .wrap_err("Failed to create signer from private key")?;

    let encoded_solution = encode_solution(slow_solution.clone(), &chain.name)?;
    encode_tycho_router_call(
        chain.chain_id(),
        encoded_solution,
        &slow_solution,
        native_address,
        signer,
    )
    .wrap_err("Failed to encode slow chain transaction")
}

pub(crate) fn build_fast_chain_transaction(
    arb_signal: &CrossChainSingleHop,
    private_key: &str,
) -> eyre::Result<Transaction> {
    let fast_solution = create_fast_solution(arb_signal)?.clone();
    let chain = &arb_signal.fast_chain;
    let native_address = chain.name.native_token().address.clone();
    let signer = PrivateKeySigner::from_str(&private_key)
        .wrap_err("Failed to create signer from private key")?;

    let encoded_solution = encode_solution(fast_solution.clone(), &chain.name)?;
    encode_tycho_router_call(
        chain.chain_id(),
        encoded_solution,
        &fast_solution,
        native_address,
        signer,
    )
    .wrap_err("Failed to encode fast chain transaction")
}

pub(crate) fn encode_solution(solution: Solution, chain: &Chain) -> eyre::Result<EncodedSolution> {
    // Initialize the encoder
    let encoder = TychoRouterEncoderBuilder::new()
        .chain(chain.clone())
        .user_transfer_type(UserTransferType::TransferFrom)
        .build()
        .expect("Failed to build encoder");

    // Encode the solution
    let encoded_solution = encoder
        .encode_solutions(vec![solution.clone()])
        .expect("Failed to encode router calldata")[0]
        .clone();

    Ok(encoded_solution)
}

fn sign_permit(
    chain_id: u64,
    permit_single: &models::PermitSingle,
    signer: PrivateKeySigner,
) -> Result<Signature, EncodingError> {
    let permit2_address = Address::from_str("0x000000000022D473030F116dDEE9F6B43aC78BA3")
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
    let signature = sign_permit(chain_id, &p, signer)?;
    let given_amount = biguint_to_u256(&solution.given_amount);
    let min_amount_out = biguint_to_u256(&solution.checked_amount);
    let given_token = Address::from_slice(&solution.given_token);
    let checked_token = Address::from_slice(&solution.checked_token);
    let receiver = Address::from_slice(&solution.receiver);

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

pub(crate) fn create_slow_solution(arb_signal: &CrossChainSingleHop) -> eyre::Result<Solution> {
    // Create the solution using the encoder
    let slow_solution = create_solution(
        ProtocolComponent::default(),          // todo: use the correct component
        arb_signal.slow_sim.token_in.clone(),  // assume asset_a is the sell token
        arb_signal.slow_sim.token_out.clone(), // assume asset_b is the buy token
        arb_signal.slow_sim.amount_in.clone(),
        arb_signal.slow_sim.amount_out.clone(),
        "0x0000000000000000000000000000000000000000".into(), // user address
    );

    Ok(slow_solution)
}

pub(crate) fn create_fast_solution(arb_signal: &CrossChainSingleHop) -> eyre::Result<Solution> {
    // Create the solution using the encoder
    let solution = create_solution(
        ProtocolComponent::default(),          // todo: use the correct component
        arb_signal.fast_sim.token_in.clone(),  // assume asset_b is the sell token
        arb_signal.fast_sim.token_out.clone(), // assume asset_a is the buy token
        arb_signal.slow_sim.amount_in.clone(),
        arb_signal.slow_sim.amount_out.clone(),
        "0x0000000000000000000000000000000000000000".into(), // user address
    );
    Ok(solution)
}
fn create_solution(
    component: ProtocolComponent,
    sell_token: Token,
    buy_token: Token,
    amount_in: BigUint,
    min_amount_out: BigUint,
    user_address: Bytes,
) -> Solution {
    // Prepare data to encode. First we need to create a swap object
    let simple_swap = Swap::new(
        component,
        sell_token.address.clone(),
        buy_token.address.clone(),
        // Split defines the fraction of the amount to be swapped. A value of 0 indicates 100% of
        // the amount or the total remaining balance.
        0f64,
        None,
    );

    Solution {
        sender: user_address.clone(),
        receiver: user_address,
        given_token: sell_token.address,
        given_amount: amount_in,
        checked_token: buy_token.address,
        exact_out: false, // it's an exact in solution
        checked_amount: min_amount_out,
        swaps: vec![simple_swap],
        native_action: None,
    }
}
