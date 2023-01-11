use anyhow::Result;

use crate::app;

/// Update Kubernetes manifests from latest stable release and create a new Cue file for the new
/// release
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        // Dummy arguments to satisfy `impl` trait bound
        app::exec_in_app_path("scripts/generate-manifests.sh", [""])?;
        app::exec_in_app_path("scripts/generate-release-cue.rb", [""])
    }
}
