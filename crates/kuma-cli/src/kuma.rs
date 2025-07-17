pub(crate) struct Kuma {
    all_tokens: HashMap<Chain, HashMap<tycho_common::Bytes, Token>>,
    pair: Pair,
    chain_a: Chain,
    chain_b: Chain,
}

impl Kuma {
    pub fn new(cfg: Config, cli: Cli) -> Self {
        unimplemented!()
    }

    pub async fn run(&self) -> eyre::Result<()> {
        Ok(())
    }
}
