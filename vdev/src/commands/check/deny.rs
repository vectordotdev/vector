use anyhow::Result;

use crate::app;

/// Check for advisories, licenses, and sources for crate dependencies
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Only check licenses
    #[arg(long)]
    licenses_only: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let check = if self.licenses_only {
            "licenses"
        } else {
            "all"
        };
        app::exec(
            "cargo",
            [
                "deny",
                "--log-level",
                "error",
                "--all-features",
                "check",
                check,
            ],
            true,
        )
    }
}
