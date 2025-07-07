use tycho_common::models::Chain;

#[derive(Debug, Clone)]
pub(crate) struct ChainInfo {
    pub(crate) chain: String,
    chain_id: Chain,
    block_time: u64,
}
