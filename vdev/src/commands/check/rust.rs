use anyhow::Result;

use crate::app;

/// Check the Rust code for errors
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    #[arg(long)]
    clippy: bool,

    #[arg(value_name = "FEATURE")]
    features: Vec<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let features = self.features.join(",");
        let features = if self.features.is_empty() {
            "default,all-integration-tests"
        } else {
            &features
        };
        if self.clippy {
            app::exec(
                "cargo",
                [
                    "dylint",
                    "--all",
                    "--workspace",
                    "--",
                    // "--all-targets", BROKEN
                    "--no-default-features",
                    "--features",
                    features,
                ],
                true,
            )
        } else {
            app::exec(
                "cargo",
                [
                    "check",
                    "--workspace",
                    "--all-targets",
                    "--no-default-features",
                    "--features",
                    features,
                ],
                true,
            )
        }
    }
}
