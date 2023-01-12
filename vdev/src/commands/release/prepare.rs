use anyhow::Result;

use crate::app;

/// Update Kubernetes manifests from latest stable release and create a new Cue file for the new
/// release
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::exec_in_repo::<&str>("scripts/generate-manifests.sh", [])?;
        app::exec_in_repo::<&str>("scripts/generate-release-cue.rb", [])
    }
}
