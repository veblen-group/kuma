use std::str::FromStr as _;

use alloy::{
    network::{EthereumWallet, TransactionBuilder},
    primitives::{Address, Keccak256, U256},
    providers::{Provider as _, ProviderBuilder},
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
    sol_types::SolValue as _,
};

use color_eyre::eyre::{self, Context as _};
use core::config::Config;
use tracing::info;

#[derive(clap::Args, Debug)]
pub(crate) struct SignPermit2 {}

impl SignPermit2 {
    pub(crate) async fn run(&self, config: Config) -> eyre::Result<()> {
        let (tokens_by_chain, _) = config
            .build_addrs_and_inventory()
            .expect("Failed to parse chain assets");

        info!("Parsed {} chains from config:", tokens_by_chain.len());

        for (chain, _tokens) in &tokens_by_chain {
            info!(chain.name = %chain.name,
                        chain.id = %chain.metadata.id(),
                        "🔗 Initialized chain info from config");
        }

        let signer: PrivateKeySigner = config
            .private_key
            .parse()
            .wrap_err("Failed to parse private key")?;

        let wallet = EthereumWallet::new(signer.clone());
        let approve_function_signature = "approve(address,uint256)";
        for (chain, tokens) in tokens_by_chain.iter() {
            let args = (
                Address::from_str(&chain.permit2_address).expect("Couldn't convert to address"),
                U256::MAX, // Approve maximum amount
            );
            let call_data = encode_input(approve_function_signature, args.abi_encode());
            let provider = ProviderBuilder::new()
                .wallet(wallet.clone())
                .connect_http(chain.rpc_url.parse().wrap_err("Failed to parse RPC URL")?);

            for address in tokens.keys() {
                let tx = TransactionRequest::default()
                    .with_to(
                        address
                            .to_string()
                            .parse()
                            .wrap_err("Failed to parse token address")?,
                    )
                    .with_chain_id(chain.chain_id())
                    .with_input(call_data.clone());

                let builder = provider.send_transaction(tx).await?;
                let pending_tx = builder.register().await?;
                let tx_hash = pending_tx.await?;
                let receipt = provider
                    .get_transaction_receipt(tx_hash)
                    .await?
                    .expect("Transaction receipt not found");
                info!(
                    "Transaction successful with hash: {}",
                    receipt.transaction_hash
                );
            }
        }

        Ok(())
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
