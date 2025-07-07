use serde::{Deserialize, Serialize};
use tycho_common::models::Chain;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChainInfo {
    pub(crate) chain: Chain,
    pub(crate) chain_id: u64,
    pub(crate) block_time: u64,
    pub(crate) rpc_url: String,
}
