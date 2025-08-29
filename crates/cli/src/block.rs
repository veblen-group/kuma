use color_eyre::eyre;

use crate::kuma;

#[derive(clap::Args, Debug)]
pub(crate) struct GetBlock;

impl GetBlock {
    pub(crate) async fn run(&self, kuma: &kuma::Kuma) -> eyre::Result<()> {
        Ok(())
    }
}
