use std::collections::HashMap;

use tracing::info;
use tycho_common::{Bytes, models::token::Token};
use tycho_simulation::tycho_client::{HttpRPCClient, rpc::RPCClient as _};

/// Loads all tokens from Tycho and returns them as a Hashmap of address->Token.
///
/// # Arguments
///
/// * `tycho_url` - The URL of the Tycho RPC (do not include the url prefix e.g. 'https://').
/// * `no_tls` - Whether to use HTTP instead of HTTPS.
/// * `auth_key` - The API key to use for authentication.
/// * `chain` - The chain to load tokens from.
/// * `min_quality` - The minimum quality of tokens to load. Defaults to 100 if not provided.
/// * `max_days_since_last_trade` - The max number of days since the token was last traded. Defaults
///   are chain specific and applied if not provided.
pub async fn load_all_tokens(
    tycho_url: &str,
    no_tls: bool,
    auth_key: Option<&str>,
    chain: tycho_common::models::Chain,
    min_quality: Option<i32>,
    max_days_since_last_trade: Option<u64>,
) -> HashMap<Bytes, Token> {
    info!(chain = %chain,"Loading tokens from Tycho...");
    let rpc_url = if no_tls {
        format!("http://{tycho_url}")
    } else {
        format!("https://{tycho_url}")
    };
    let rpc_client = HttpRPCClient::new(rpc_url.as_str(), auth_key).unwrap();

    // Chain specific defaults for special case chains. Otherwise defaults to 42 days.
    let default_min_days = HashMap::from([(tycho_common::models::Chain::Base, 1_u64)]);

    #[allow(clippy::mutable_key_type)]
    rpc_client
        .get_all_tokens(
            chain.into(),
            min_quality.or(Some(100)),
            max_days_since_last_trade.or(default_min_days.get(&chain).or(Some(&42)).copied()),
            3_000,
        )
        .await
        .expect("Unable to load tokens")
        .into_iter()
        .map(|token| {
            let token_clone = token.clone();
            (
                token.address.clone(),
                token.try_into().unwrap_or_else(|_| {
                    panic!("Couldn't convert {token_clone:?} into ERC20 token.")
                }),
            )
        })
        .collect::<HashMap<_, Token>>()
}
