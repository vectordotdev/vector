use anyhow::Result;

use crate::{app, util::ChainArgs as _};

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
        let tool = if self.clippy { "clippy" } else { "check" };
        let args = if self.clippy {
            vec!["--", "-D", "warnings"]
        } else {
            Vec::default()
        };
        app::exec(
            "cargo",
            [
                tool,
                "--workspace",
                "--all-targets",
                "--no-default-features",
                "--features",
                features,
            ]
            .chain_args(args),
            true,
        )
    }
}
