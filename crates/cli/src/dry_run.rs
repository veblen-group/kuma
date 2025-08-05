use std::process::ExitCode;

use color_eyre::eyre;

use crate::StrategyArgs;

#[derive(clap::Args, Debug)]
pub(crate) struct DryRun {
    #[clap(flatten)]
    args: StrategyArgs,
}

impl DryRun {
    #[allow(dead_code)]
    pub(crate) async fn run(&self) -> eyre::Result<ExitCode> {
        unimplemented!("DryRun command is not implemented yet");
    }
}
