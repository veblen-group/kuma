use std::str::FromStr as _;
use std::u64;

use alloy::consensus::EthereumTxEnvelope;
use alloy::network::EthereumWallet;
use alloy::primitives::{Address as alloyAddress, Bytes as AlloyBytes, Keccak256};
use alloy::providers::Provider as _;
use alloy::providers::ext::AnvilApi;
use alloy::rpc::types::{TransactionInput, TransactionReceipt, TransactionRequest};
use alloy::signers::Signature;
use alloy::signers::{SignerSync, local::PrivateKeySigner};
use alloy::sol_types::{SolStruct, SolValue, eip712_domain};
use color_eyre::eyre::{self, Context as _, ContextCompat, Ok};
use num_bigint::BigUint;
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

pub struct Trade {
    slow_tx_req: UnsignedTransaction,
    fast_tx_req: UnsignedTransaction,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignedTransaction {
    tx: EthereumTxEnvelope<alloy::consensus::TxEip4844Variant>,
}

impl Trade {
    pub(crate) fn new(slow: UnsignedTransaction, fast: UnsignedTransaction) -> Self {
        Trade {
            slow_tx_req: slow,
            fast_tx_req: fast,
        }
    }

    pub fn slow_tx(&self) -> &UnsignedTransaction {
        &self.slow_tx_req
    }

    pub fn fast_tx(&self) -> &UnsignedTransaction {
        &self.fast_tx_req
    }

    // Prepare the trade by creating the transaction requests for both chains
    pub async fn prepare(&self) -> eyre::Result<(SignedTransaction, SignedTransaction)> {
        let slow_tx_request = get_tx_request(&self.slow_tx(), &self.slow_tx().chain)
            .await
            .wrap_err("Failed to create transaction request for slow chain")?;
        let fast_tx_request = get_tx_request(self.fast_tx(), &self.fast_tx().chain)
            .await
            .wrap_err("Failed to create transaction request for fast chain")?;
        Ok((slow_tx_request, fast_tx_request))
    }

    // Execute the trade by sending the transactions to their respective chains
    pub async fn promote(self) -> eyre::Result<(TransactionReceipt, TransactionReceipt)> {
        let slow_tx = execute_tx(self.slow_tx(), &self.slow_tx().chain)
            .await
            .wrap_err("Failed to execute slow transaction")?;

        let fast_tx = execute_tx(self.fast_tx(), &self.fast_tx().chain)
            .await
            .wrap_err("Failed to execute fast transaction")?;

        Ok((slow_tx, fast_tx))
    }
}

pub struct UnsignedTransaction {
    tx: TransactionRequest,
    chain: Chain,
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
        Ok(UnsignedTransaction {
            tx: tx_request,
            chain: chain.clone(),
        })
    }
}

// used for dry run
pub async fn get_tx_request(
    transaction: &UnsignedTransaction,
    chain: &Chain,
) -> eyre::Result<SignedTransaction> {
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
) -> eyre::Result<TransactionReceipt> {
    let wallet = EthereumWallet::new(chain.signer().clone());
    let provider = alloy::providers::ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(chain.rpc_url.parse().wrap_err("Invalid RPC URL")?);

    provider.anvil_set_logging(true).await.ok();

    let pending_tx = provider
        .send_transaction(transaction.tx.clone())
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
        // Split defines the fraction of the amount to be swapped. A value of 0 indicates 100% of
        // the amount or the total remaining balance.
        0f64,
        None,
        None, // protocol_sim
        None, // output_amount
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
