use std::process::Command;

use anyhow::Result;

use crate::app::CommandExt as _;

/// Update Kubernetes manifests from latest stable release and create a new Cue file for the new
/// release
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        Command::script("generate-manifests.sh")
            .in_repo()
            .check_run()?;
        Command::script("generate-release-cue.rb")
            .in_repo()
            .check_run()
    }
}
