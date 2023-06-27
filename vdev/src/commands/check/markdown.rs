use anyhow::Result;

use crate::app;

/// Check that markdown is styled properly
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::exec(
            "markdownlint",
            [
                "--config",
                "scripts/.markdownlintrc",
                "--ignore",
                "scripts/node_modules",
                "--ignore",
                "website/node_modules",
                "--ignore",
                "target",
                // The following is a submodule and should be excluded from formatting checks
                "--ignore",
                "lib/opentelemetry-proto/src/proto/opentelemetry-proto",
                ".",
            ],
            true,
        )
    }
}
