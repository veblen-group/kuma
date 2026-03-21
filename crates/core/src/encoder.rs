//! Transaction encoding and signing for Tycho router execution.
//!
//! Converts swap solutions into signed Ethereum transactions with Permit2 approvals,
//! handling EIP712 signature generation and ABI encoding for the Tycho router contract.

use std::str::FromStr as _;

use alloy::consensus::EthereumTxEnvelope;
use alloy::network::EthereumWallet;
use alloy::primitives::{Address as alloyAddress, Bytes as AlloyBytes, Keccak256};
use alloy::providers::Provider as _;
use alloy::providers::ext::AnvilApi;
use alloy::rpc::types::{TransactionInput, TransactionReceipt, TransactionRequest};
use alloy::signers::Signature;
use alloy::signers::{SignerSync, local::PrivateKeySigner};
use alloy::sol_types::{SolStruct, SolValue, eip712_domain};
use color_eyre::eyre::{self, Context as _, ContextCompat, OptionExt};
use num_bigint::BigUint;
use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};
use tracing::trace;
use tycho_common::Bytes;

use tycho_execution::encoding::errors::EncodingError;
use tycho_execution::encoding::evm::approvals::permit2::PermitSingle;
use tycho_execution::encoding::evm::encoder_builders::TychoRouterEncoderBuilder;
use tycho_execution::encoding::evm::utils::biguint_to_u256;
use tycho_execution::encoding::models::{
    self, EncodedSolution, Solution, Swap as TychoSwap, Transaction as TychoTransaction,
    UserTransferType,
};
use tycho_simulation::protocol::models::ProtocolComponent;

use crate::chain::Chain;
use crate::strategy::Swap;

#[derive(Debug, Serialize, Deserialize)]
pub struct SignedTransaction {
    tx: EthereumTxEnvelope<alloy::consensus::TxEip4844Variant>,
}

#[derive(Debug, Clone)]
pub struct UnsignedTransaction {
    tx: TransactionRequest,
}

impl UnsignedTransaction {
    pub(crate) fn try_from_solution(solution: &Solution, chain: &Chain) -> eyre::Result<Self> {
        let encoded_solution = encode_solution(solution.clone(), chain)?;
        let native_address = Bytes::from(chain.name.native_token().address.as_ref());
        let tx = encode_tycho_router_call(
            chain.chain_id(),
            encoded_solution,
            solution,
            native_address,
            chain.signer().clone(),
        )?;
        let tx_request = TransactionRequest::default()
            .to(alloyAddress::from_slice(&tx.to))
            .input(TransactionInput {
                input: Some(AlloyBytes::from(tx.data.clone())),
                data: None,
            })
            .value(biguint_to_u256(&tx.value));
        Ok(UnsignedTransaction { tx: tx_request })
    }
}

// used for dry run
pub async fn get_tx_request(
    transaction: &UnsignedTransaction,
    chain: &Chain,
) -> eyre::Result<SignedTransaction> {
    // TODO: long-lived provider isntead of creating it every time
    let wallet = EthereumWallet::new(chain.signer().clone());
    let provider = alloy::providers::ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(chain.rpc_url.parse().wrap_err("Invalid RPC URL")?);

    provider.anvil_set_logging(true).await.ok();

    let tx = provider
        .fill(transaction.tx.clone())
        .await
        .wrap_err("failed filling tx")?
        .try_into_envelope()?;
    Ok(SignedTransaction { tx })
}

// used for execution
pub async fn execute_tx(
    transaction: &UnsignedTransaction,
    chain: &Chain,
    base_fee: u64,
    gas_cost: &BigUint,
) -> eyre::Result<TransactionReceipt> {
    // TODO: long-lived provider isntead of creating it every time
    let wallet = EthereumWallet::new(chain.signer().clone());
    let provider = alloy::providers::ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(chain.rpc_url.parse().wrap_err("Invalid RPC URL")?);

    provider.anvil_set_logging(true).await.ok();

    let gas_limit = gas_cost
        .to_u64()
        .ok_or_eyre("failed converting gas cost to u64")?;
    let pending_tx = provider
        .send_transaction(
            transaction
                .tx
                .clone()
                .gas_price(base_fee as u128 * 2) // TODO: this seems stupid
                .gas_limit(gas_limit * 2),
        )
        .await
        .wrap_err("failed sending transaction")?;

    let receipt = pending_tx
        .get_receipt()
        .await
        .wrap_err("failed getting receipt")?;
    trace!("Transaction mined in block {:?}", receipt.block_number);
    Ok(receipt)
}

pub(crate) fn encode_tycho_router_call(
    chain_id: u64,
    encoded_solution: EncodedSolution,
    solution: &Solution,
    native_address: Bytes,
    signer: PrivateKeySigner,
) -> eyre::Result<TychoTransaction> {
    let p = encoded_solution
        .permit
        .wrap_err("Permit object must be set")?;
    let permit = PermitSingle::try_from(&p)
        .map_err(|e| EncodingError::InvalidInput(format!("Invalid permit: {e}")))?;
    trace!("Signing permit2 approval: {permit:?}");
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
    Ok(TychoTransaction {
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

pub(crate) fn create_solution(
    component: ProtocolComponent,
    swap: &Swap,
    signer: PrivateKeySigner,
) -> eyre::Result<Solution> {
    let signer_address_bytes =
        tycho_common::models::Address::from_str(signer.address().to_string().as_str())
            .wrap_err("Invalid signer address")?;
    // Convert tycho_simulation bytes to tycho_common bytes by converting through hex string
    let simple_swap = TychoSwap::new(
        component,
        swap.token_in.address.clone(),
        swap.token_out.address.clone(),
    );

    Ok(Solution {
        sender: signer_address_bytes.clone(),
        receiver: signer_address_bytes.clone(),
        given_token: swap.token_in.address.clone(),
        given_amount: swap.amount_in.clone(),
        checked_token: swap.token_out.address.clone(),
        exact_out: false, // it's an exact in solution
        checked_amount: swap.amount_out.clone(),
        swaps: vec![simple_swap],
        native_action: None,
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
