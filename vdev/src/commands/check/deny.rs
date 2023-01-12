use anyhow::Result;

use crate::app;

/// Check advisories, licenses, and sources for crate dependencies
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::exec_in_repo(
            "cargo",
            [
                "deny",
                "--log-level",
                "error",
                "--all-features",
                "check",
                "all",
            ],
        )
    }
}
