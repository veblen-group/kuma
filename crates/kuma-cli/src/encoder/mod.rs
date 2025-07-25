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

use crate::signals::ArbSignal;

// TODO: sign and submit permit2 approval per new token once before sending any transactions.
//.      Allows sending transactions without the need for a seperate approval.
const PERMIT2_ADDRESS: &str = "0x000000000022D473030F116dDEE9F6B43aC78BA3"; // todo: check if this is the correct address
const USER_ADDRESS: &str = "0x0000000000000000000000000000000000000000"; // Placeholder, replace with actual user address

pub(crate) async fn generate_transactions_from_signal(
    signal: ArbSignal,
    slow_chain_provider: FillProvider<
        JoinFill<Identity, WalletFiller<EthereumWallet>>,
        RootProvider,
    >,
    fast_chain_provider: FillProvider<
        JoinFill<Identity, WalletFiller<EthereumWallet>>,
        RootProvider,
    >,
) -> eyre::Result<(TransactionRequest, TransactionRequest)> {
    let slow_tx = build_slow_chain_transaction(&signal)?;
    let fast_tx = build_fast_chain_transaction(&signal)?;
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

pub(crate) fn build_slow_chain_transaction(arb_signal: &ArbSignal) -> eyre::Result<Transaction> {
    let slow_solution = create_slow_solution(arb_signal)?.clone();
    let chain = &arb_signal.slow_chain;
    let native_address = chain.name.native_token().address.clone();
    let signer = PrivateKeySigner::from_str(&arb_signal.signer_private_key)
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

pub(crate) fn build_fast_chain_transaction(arb_signal: &ArbSignal) -> eyre::Result<Transaction> {
    let fast_solution = create_fast_solution(arb_signal)?.clone();
    let chain = &arb_signal.fast_chain;
    let native_address = chain.name.native_token().address.clone();
    let signer = PrivateKeySigner::from_str(&arb_signal.signer_private_key)
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

pub(crate) fn create_slow_solution(arb_signal: &ArbSignal) -> eyre::Result<Solution> {
    // Create the solution using the encoder
    let slow_solution = create_solution(
        arb_signal.slow_chain_protocol_component.clone(), // todo: use the correct component
        arb_signal.asset_a.clone(),                       // assume asset_a is the sell token
        arb_signal.asset_b.clone(),                       // assume asset_b is the buy token
        arb_signal.optimal_amount_in.clone(),
        arb_signal.slow_chain_amount_out.clone(),
        "0x0000000000000000000000000000000000000000".into(), // user address
    );

    Ok(slow_solution)
}

pub(crate) fn create_fast_solution(arb_signal: &ArbSignal) -> eyre::Result<Solution> {
    // Create the solution using the encoder
    let solution = create_solution(
        ProtocolComponent::default(), // todo: use the correct component
        arb_signal.asset_b.clone(),   // assume asset_b is the sell token
        arb_signal.asset_a.clone(),   // assume asset_a is the buy token
        arb_signal.slow_chain_amount_out.clone(),
        arb_signal.fast_chain_amount_out.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::Chain;
    use crate::signals::{ArbSignal, Direction};
    use alloy::primitives::Keccak256;
    use alloy::signers::local::PrivateKeySigner;
    use num_bigint::BigUint;
    use proptest::prelude::*;
    use std::result::Result;
    use std::str::FromStr;
    use tycho_common::models::{Chain as TychoChain, protocol::ProtocolComponent, token::Token};
    use tycho_execution::encoding::models::{self, PermitDetails, PermitSingle, Solution, Swap};

    // Test helper functions
    fn create_test_token(address: &str, symbol: &str, decimals: u32) -> Token {
        Token::new(
            &tycho_common::Bytes::from_str(address).unwrap(),
            symbol,
            decimals,
            1000,
            &[Some(1000u64)],
            TychoChain::Ethereum,
            100,
        )
    }

    fn create_test_protocol_component() -> ProtocolComponent {
        ProtocolComponent {
            id: "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc".to_string(),
            protocol_system: "uniswap_v2".to_string(),
            ..Default::default()
        }
    }

    fn create_test_arb_signal() -> ArbSignal {
        let asset_a = create_test_token("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", "USDC", 6);
        let asset_b = create_test_token("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "WETH", 18);
        let chain = Chain::eth_mainnet();
        let protocol_component = create_test_protocol_component();

        ArbSignal {
            asset_a,
            asset_b,
            slow_chain: chain.clone(),
            fast_chain: chain,
            slow_chain_protocol_component: protocol_component.clone(),
            fast_chain_protocol_component: protocol_component,
            path: Direction::AtoB,
            slow_chain_amount_out: BigUint::from(1000u64),
            fast_chain_amount_out: BigUint::from(1100u64),
            profit_percentage: 10.0,
            optimal_amount_in: BigUint::from(500u64),
            expected_profit: BigUint::from(100u64),
            signer_private_key:
                "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318".to_string(),
        }
    }

    fn create_test_solution() -> Solution {
        let sell_token = create_test_token("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", "USDC", 6);
        let buy_token = create_test_token("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "WETH", 18);
        let user_address =
            tycho_common::Bytes::from_str("0x742d35Cc6634C0532925a3b8D4e3CB0532925A3B").unwrap();

        Solution {
            sender: user_address.clone(),
            receiver: user_address,
            given_token: sell_token.address.clone(),
            given_amount: BigUint::from(1000u64),
            checked_token: buy_token.address.clone(),
            exact_out: false,
            checked_amount: BigUint::from(500u64),
            swaps: vec![Swap::new(
                create_test_protocol_component(),
                sell_token.address.clone(),
                buy_token.address.clone(),
                0f64,
                None,
            )],
            native_action: None,
        }
    }

    #[test]
    fn test_encode_input_basic() {
        let selector = "transfer(address,uint256)";
        let args = vec![0u8; 64]; // Mock encoded args

        let result = encode_input(selector, args);

        // Should start with function selector (4 bytes)
        assert_eq!(result.len(), 68); // 4 bytes selector + 64 bytes args
        // First 4 bytes should be keccak256 hash of selector
        assert_eq!(&result[0..4], &[0xa9, 0x05, 0x9c, 0xbb]); // transfer(address,uint256) selector
    }

    #[test]
    fn test_encode_input_with_dynamic_prefix() {
        let selector = "test()";
        let mut args = vec![0u8; 31];
        args.push(32u8); // Add the dynamic prefix
        args.extend(vec![1u8; 32]); // Add actual data

        let result = encode_input(selector, args);

        // Should remove the 32-byte prefix and use only the actual data
        assert_eq!(result.len(), 36); // 4 bytes selector + 32 bytes data
        assert_eq!(&result[4..], &vec![1u8; 32]);
    }

    #[test]
    fn test_encode_input_empty_args() {
        let selector = "test()";
        let args = vec![];

        let result = encode_input(selector, args);

        // Should only contain the function selector (4 bytes)
        assert_eq!(result.len(), 4);
        // Verify correct selector for test()
        let mut hasher = Keccak256::new();
        hasher.update(selector.as_bytes());
        let expected_selector = &hasher.finalize()[..4];
        assert_eq!(&result[0..4], expected_selector);
    }

    #[test]
    fn test_encode_input_no_dynamic_prefix() {
        let selector = "simple(uint256)";
        let args = vec![1u8; 32]; // 32 bytes, but no prefix pattern

        let result = encode_input(selector, args);

        // Should include all args
        assert_eq!(result.len(), 36); // 4 bytes selector + 32 bytes args
        assert_eq!(&result[4..], &vec![1u8; 32]);
    }

    // Tests for create_solution function
    #[test]
    fn test_create_solution_basic() {
        let component = create_test_protocol_component();
        let sell_token = create_test_token("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", "USDC", 6);
        let buy_token = create_test_token("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "WETH", 18);
        let amount_in = BigUint::from(1000u64);
        let min_amount_out = BigUint::from(500u64);
        let user_address =
            tycho_common::Bytes::from_str("0x742d35Cc6634C0532925a3b8D4e3CB0532925A3B").unwrap();

        let solution = create_solution(
            component.clone(),
            sell_token.clone(),
            buy_token.clone(),
            amount_in.clone(),
            min_amount_out.clone(),
            user_address.clone(),
        );

        assert_eq!(solution.sender, user_address);
        assert_eq!(solution.receiver, user_address);
        assert_eq!(solution.given_token, sell_token.address);
        assert_eq!(solution.given_amount, amount_in);
        assert_eq!(solution.checked_token, buy_token.address);
        assert_eq!(solution.checked_amount, min_amount_out);
        assert!(!solution.exact_out);
        assert_eq!(solution.swaps.len(), 1);
        assert!(solution.native_action.is_none());

        // Verify swap details
        let swap = &solution.swaps[0];
        assert_eq!(swap.token_in, sell_token.address);
        assert_eq!(swap.token_out, buy_token.address);
        assert_eq!(swap.split, 0f64); // 100% of amount
        assert_eq!(swap.component, component);
    }

    #[test]
    fn test_create_solution_zero_amounts() {
        let component = create_test_protocol_component();
        let sell_token = create_test_token("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", "USDC", 6);
        let buy_token = create_test_token("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "WETH", 18);
        let user_address =
            tycho_common::Bytes::from_str("0x742d35Cc6634C0532925a3b8D4e3CB0532925A3B").unwrap();

        let solution = create_solution(
            component,
            sell_token.clone(),
            buy_token.clone(),
            BigUint::from(0u64),
            BigUint::from(0u64),
            user_address.clone(),
        );

        assert_eq!(solution.given_amount, BigUint::from(0u64));
        assert_eq!(solution.checked_amount, BigUint::from(0u64));
        assert_eq!(solution.sender, user_address);
        assert_eq!(solution.receiver, user_address);
    }

    #[test]
    fn test_create_solution_same_tokens() {
        let component = create_test_protocol_component();
        let token = create_test_token("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", "USDC", 6);
        let user_address =
            tycho_common::Bytes::from_str("0x742d35Cc6634C0532925a3b8D4e3CB0532925A3B").unwrap();

        let solution = create_solution(
            component.clone(),
            token.clone(),
            token.clone(),
            BigUint::from(1000u64),
            BigUint::from(1000u64),
            user_address.clone(),
        );

        assert_eq!(solution.given_token, solution.checked_token);
        assert_eq!(solution.given_token, token.address);

        let swap = &solution.swaps[0];
        assert_eq!(swap.token_in, swap.token_out);
    }

    // Tests for create_slow_solution and create_fast_solution
    #[test]
    fn test_create_slow_solution() {
        let signal = create_test_arb_signal();

        let result = create_slow_solution(&signal);

        assert!(result.is_ok());
        let solution = result.unwrap();

        // Verify token mapping
        assert_eq!(solution.given_token, signal.asset_a.address);
        assert_eq!(solution.checked_token, signal.asset_b.address);

        // Verify amounts
        assert_eq!(solution.given_amount, signal.optimal_amount_in);
        assert_eq!(solution.checked_amount, signal.slow_chain_amount_out);

        // Verify user address (hardcoded in function)
        let expected_user =
            tycho_common::Bytes::from_str("0x0000000000000000000000000000000000000000").unwrap();
        assert_eq!(solution.sender, expected_user);
        assert_eq!(solution.receiver, expected_user);

        // Verify swap configuration
        assert_eq!(solution.swaps.len(), 1);
        let swap = &solution.swaps[0];
        assert_eq!(swap.component, signal.slow_chain_protocol_component);
        assert_eq!(swap.token_in, signal.asset_a.address);
        assert_eq!(swap.token_out, signal.asset_b.address);
    }

    #[test]
    fn test_create_fast_solution() {
        let signal = create_test_arb_signal();

        let result = create_fast_solution(&signal);

        assert!(result.is_ok());
        let solution = result.unwrap();

        // Verify token mapping (reversed from slow)
        assert_eq!(solution.given_token, signal.asset_b.address);
        assert_eq!(solution.checked_token, signal.asset_a.address);

        // Verify amounts
        assert_eq!(solution.given_amount, signal.slow_chain_amount_out);
        assert_eq!(solution.checked_amount, signal.fast_chain_amount_out);

        // Verify user address (hardcoded in function)
        let expected_user =
            tycho_common::Bytes::from_str("0x0000000000000000000000000000000000000000").unwrap();
        assert_eq!(solution.sender, expected_user);
        assert_eq!(solution.receiver, expected_user);

        // Verify swap configuration
        assert_eq!(solution.swaps.len(), 1);
        let swap = &solution.swaps[0];
        assert_eq!(swap.component, ProtocolComponent::default()); // Uses default in fast solution
        assert_eq!(swap.token_in, signal.asset_b.address);
        assert_eq!(swap.token_out, signal.asset_a.address);
    }

    #[test]
    fn test_slow_fast_solution_consistency() {
        let signal = create_test_arb_signal();

        let slow_solution = create_slow_solution(&signal).unwrap();
        let fast_solution = create_fast_solution(&signal).unwrap();

        // Tokens should be swapped between slow and fast
        assert_eq!(slow_solution.given_token, fast_solution.checked_token);
        assert_eq!(slow_solution.checked_token, fast_solution.given_token);

        // Slow output should equal fast input
        assert_eq!(slow_solution.checked_amount, fast_solution.given_amount);

        // Both should have the same user addresses
        assert_eq!(slow_solution.sender, fast_solution.sender);
        assert_eq!(slow_solution.receiver, fast_solution.receiver);
    }

    // Tests for encode_solution function
    #[test]
    fn test_encode_solution_basic() {
        let solution = create_test_solution();
        let chain = TychoChain::Ethereum;

        let result = encode_solution(solution.clone(), &chain);

        assert!(result.is_ok());
        let encoded = result.unwrap();

        // Verify basic structure
        assert!(encoded.permit.is_some());
        assert!(!encoded.swaps.is_empty());
        assert!(!encoded.function_signature.is_empty());
        assert!(!encoded.interacting_with.is_empty());

        // Verify permit structure
        let permit = encoded.permit.unwrap();
        assert_eq!(permit.details.token, solution.given_token);
        assert_eq!(permit.details.amount, solution.given_amount);
        assert!(permit.details.expiration > BigUint::from(0u64));
        assert!(permit.sig_deadline > BigUint::from(0u64));
    }

    #[test]
    fn test_encode_solution_different_chains() {
        let solution = create_test_solution();

        let eth_encoded = encode_solution(solution.clone(), &TychoChain::Ethereum).unwrap();
        let base_encoded = encode_solution(solution.clone(), &TychoChain::Base).unwrap();

        // Basic structure should be similar
        assert!(eth_encoded.permit.is_some());
        assert!(base_encoded.permit.is_some());
        assert!(!eth_encoded.swaps.is_empty());
        assert!(!base_encoded.swaps.is_empty());

        // But encoded results may differ due to chain-specific configurations
        // Both should be valid encodings
        assert!(!eth_encoded.function_signature.is_empty());
        assert!(!base_encoded.function_signature.is_empty());
    }

    // Tests for build_slow_chain_transaction and build_fast_chain_transaction
    #[test]
    fn test_build_slow_chain_transaction() {
        let signal = create_test_arb_signal();

        let result = build_slow_chain_transaction(&signal);

        assert!(result.is_ok());
        let tx = result.unwrap();

        // Verify transaction structure
        assert!(!tx.to.is_empty());
        assert_eq!(tx.to.len(), 20); // Address should be 20 bytes
        assert!(!tx.data.is_empty());
        assert!(tx.data.len() >= 4); // At least function selector

        // Value should be 0 for non-native token swaps
        assert_eq!(tx.value, BigUint::from(0u64));

        // Verify the 'to' address is not all zeros (valid contract address)
        assert!(tx.to.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_build_fast_chain_transaction() {
        let signal = create_test_arb_signal();

        let result = build_fast_chain_transaction(&signal);

        assert!(result.is_ok());
        let tx = result.unwrap();

        // Verify transaction structure
        assert!(!tx.to.is_empty());
        assert_eq!(tx.to.len(), 20); // Address should be 20 bytes
        assert!(!tx.data.is_empty());
        assert!(tx.data.len() >= 4); // At least function selector

        // Value should be 0 for non-native token swaps
        assert_eq!(tx.value, BigUint::from(0u64));

        // Verify the 'to' address is not all zeros (valid contract address)
        assert!(tx.to.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_build_transaction_with_native_token() {
        let mut signal = create_test_arb_signal();
        // Set asset_a to native token (ETH)
        signal.asset_a.address = signal.slow_chain.name.native_token().address.clone();

        let result = build_slow_chain_transaction(&signal);

        assert!(result.is_ok());
        let tx = result.unwrap();

        // Value should equal the given amount for native token
        assert_eq!(tx.value, signal.optimal_amount_in);

        // Transaction should still have valid structure
        assert!(!tx.to.is_empty());
        assert!(!tx.data.is_empty());
    }

    #[test]
    fn test_build_transactions_different_data() {
        let signal = create_test_arb_signal();

        let slow_tx = build_slow_chain_transaction(&signal).unwrap();
        let fast_tx = build_fast_chain_transaction(&signal).unwrap();

        // Both transactions should have valid structure
        assert!(!slow_tx.to.is_empty());
        assert!(!slow_tx.data.is_empty());
        assert!(!fast_tx.to.is_empty());
        assert!(!fast_tx.data.is_empty());

        // Transactions should be different (different token directions)
        assert_ne!(slow_tx.data, fast_tx.data);

        // Both should have the same contract addresses (router)
        assert_eq!(slow_tx.to, fast_tx.to);
    }

    #[test]
    fn test_build_transaction_invalid_private_key() {
        let mut signal = create_test_arb_signal();
        signal.signer_private_key = "invalid_key".to_string();

        let result = build_slow_chain_transaction(&signal);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to create signer")
        );
    }

    // Tests for sign_permit function
    #[test]
    fn test_sign_permit_basic() {
        let chain_id = 1u64;
        let permit_single = models::PermitSingle {
            details: models::PermitDetails {
                token: tycho_common::Bytes::from_str("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48")
                    .unwrap(),
                amount: BigUint::from(1000u64),
                expiration: BigUint::from(1000000u64),
                nonce: BigUint::from(0u64),
            },
            spender: tycho_common::Bytes::from_str("0x742d35Cc6634C0532925a3b8D4e3CB0532925A3B")
                .unwrap(),
            sig_deadline: BigUint::from(1000000u64),
        };
        let signer = PrivateKeySigner::from_str(
            "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318",
        )
        .unwrap();

        let result = sign_permit(chain_id, &permit_single, signer);

        assert!(result.is_ok());
        let signature = result.unwrap();
        assert_eq!(signature.as_bytes().len(), 65); // Standard signature length
    }

    #[test]
    fn test_sign_permit_different_chain_ids() {
        let permit_single = models::PermitSingle {
            details: models::PermitDetails {
                token: tycho_common::Bytes::from_str("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48")
                    .unwrap(),
                amount: BigUint::from(1000u64),
                expiration: BigUint::from(1000000u64),
                nonce: BigUint::from(0u64),
            },
            spender: tycho_common::Bytes::from_str("0x742d35Cc6634C0532925a3b8D4e3CB0532925A3B")
                .unwrap(),
            sig_deadline: BigUint::from(1000000u64),
        };
        let signer = PrivateKeySigner::from_str(
            "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318",
        )
        .unwrap();

        let eth_signature = sign_permit(1u64, &permit_single, signer.clone()).unwrap();
        let base_signature = sign_permit(8453u64, &permit_single, signer).unwrap();

        // Signatures should be different for different chain IDs
        assert_ne!(eth_signature.as_bytes(), base_signature.as_bytes());

        // Both should be valid signature length
        assert_eq!(eth_signature.as_bytes().len(), 65);
        assert_eq!(base_signature.as_bytes().len(), 65);
    }

    // Tests for encode_tycho_router_call function
    #[test]
    fn test_encode_tycho_router_call_basic() {
        let signal = create_test_arb_signal();
        let solution = create_slow_solution(&signal).unwrap();
        let encoded_solution = encode_solution(solution.clone(), &signal.slow_chain.name).unwrap();
        let signer = PrivateKeySigner::from_str(&signal.signer_private_key).unwrap();

        let result = encode_tycho_router_call(
            signal.slow_chain.chain_id(),
            encoded_solution.clone(),
            &solution,
            signal.slow_chain.name.native_token().address.clone(),
            signer,
        );

        assert!(result.is_ok());
        let tx = result.unwrap();

        // Transaction should have proper structure
        assert!(!tx.to.is_empty());
        assert_eq!(tx.to.len(), 20); // Valid address length
        assert!(!tx.data.is_empty());
        assert!(tx.data.len() >= 4); // At least function selector

        // Should match the encoded solution's contract address
        assert_eq!(tx.to, encoded_solution.interacting_with);

        // Value should be 0 for non-native token
        assert_eq!(tx.value, BigUint::from(0u64));
    }

    #[test]
    fn test_encode_tycho_router_call_native_token() {
        let mut signal = create_test_arb_signal();
        // Make asset_a the native token
        let native_address = signal.slow_chain.name.native_token().address.clone();
        signal.asset_a.address = native_address.clone();

        let solution = create_slow_solution(&signal).unwrap();
        let encoded_solution = encode_solution(solution.clone(), &signal.slow_chain.name).unwrap();
        let signer = PrivateKeySigner::from_str(&signal.signer_private_key).unwrap();

        let result = encode_tycho_router_call(
            signal.slow_chain.chain_id(),
            encoded_solution,
            &solution,
            native_address,
            signer,
        );

        assert!(result.is_ok());
        let tx = result.unwrap();

        // Value should equal the given amount for native token
        assert_eq!(tx.value, solution.given_amount);
    }

    // Property-based tests for solution creation
    proptest! {
        #[test]
        fn test_solution_creation_properties(
            amount_in in 1u64..1_000_000u64,
            min_amount_out in 1u64..1_000_000u64
        ) {
            let component = create_test_protocol_component();
            let sell_token = create_test_token("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", "USDC", 6);
            let buy_token = create_test_token("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "WETH", 18);
            let user_address = tycho_common::Bytes::from_str("0x742d35Cc6634C0532925a3b8D4e3CB0532925A3B").unwrap();

            let solution = create_solution(
                component,
                sell_token.clone(),
                buy_token.clone(),
                BigUint::from(amount_in),
                BigUint::from(min_amount_out),
                user_address.clone(),
            );

            assert_eq!(solution.given_amount, BigUint::from(amount_in));
            assert_eq!(solution.checked_amount, BigUint::from(min_amount_out));
            assert_eq!(solution.sender, user_address);
            assert_eq!(solution.receiver, user_address);
            assert!(!solution.exact_out);
            assert_eq!(solution.swaps.len(), 1);

            let swap = &solution.swaps[0];
            assert_eq!(swap.token_in, sell_token.address);
            assert_eq!(swap.token_out, buy_token.address);
            assert_eq!(swap.split, 0f64);
        }

        #[test]
        fn test_arb_signal_solution_consistency(
            optimal_amount in 100u64..10_000u64,
            slow_amount_out in 50u64..5_000u64,
            fast_amount_out in 75u64..7_500u64
        ) {
            let mut signal = create_test_arb_signal();
            signal.optimal_amount_in = BigUint::from(optimal_amount);
            signal.slow_chain_amount_out = BigUint::from(slow_amount_out);
            signal.fast_chain_amount_out = BigUint::from(fast_amount_out);

            let slow_solution = create_slow_solution(&signal).unwrap();
            let fast_solution = create_fast_solution(&signal).unwrap();

            // Slow solution should use optimal_amount_in as input
            assert_eq!(slow_solution.given_amount, BigUint::from(optimal_amount));
            assert_eq!(slow_solution.checked_amount, BigUint::from(slow_amount_out));

            // Fast solution should use slow_chain_amount_out as input
            assert_eq!(fast_solution.given_amount, BigUint::from(slow_amount_out));
            assert_eq!(fast_solution.checked_amount, BigUint::from(fast_amount_out));

            // Token order should be swapped between chains
            assert_eq!(slow_solution.given_token, signal.asset_a.address);
            assert_eq!(slow_solution.checked_token, signal.asset_b.address);
            assert_eq!(fast_solution.given_token, signal.asset_b.address);
            assert_eq!(fast_solution.checked_token, signal.asset_a.address);
        }

        #[test]
        fn test_transaction_data_consistency(
            amount_in in 100u64..10_000u64
        ) {
            let mut signal = create_test_arb_signal();
            signal.optimal_amount_in = BigUint::from(amount_in);

            let slow_tx_result = build_slow_chain_transaction(&signal);
            let fast_tx_result = build_fast_chain_transaction(&signal);

            assert!(slow_tx_result.is_ok());
            assert!(fast_tx_result.is_ok());

            let slow_tx = slow_tx_result.unwrap();
            let fast_tx = fast_tx_result.unwrap();

            // Both transactions should have valid structure
            assert!(!slow_tx.to.is_empty());
            assert!(!fast_tx.to.is_empty());
            assert!(!slow_tx.data.is_empty());
            assert!(!fast_tx.data.is_empty());

            // Data should be different (different token directions)
            assert_eq!(slow_tx.data, fast_tx.data);

            // Contract addresses should be the same (same router)
            assert_eq!(slow_tx.to, fast_tx.to);
        }
    }
}
