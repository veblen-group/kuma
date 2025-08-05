use std::process::ExitCode;

use color_eyre::eyre;

use crate::StrategyArgs;

#[derive(clap::Args, Debug)]
pub(crate) struct Execute {
    #[clap(flatten)]
    args: StrategyArgs,
}

impl Execute {
    #[allow(dead_code)]
    pub(crate) async fn run(&self) -> eyre::Result<ExitCode> {
        unimplemented!("Execute command is not implemented yet");
    }
}
