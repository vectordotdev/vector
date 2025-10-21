use anyhow::Result;
use clap::Args;

use crate::testing::integration::ComposeTestLocalConfig;

/// Stop an integration test environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The integration name to stop
    integration: String,

    /// If true, remove the runner container compiled with all integration test features
    #[arg(short = 'a', long)]
    build_all: bool,

    /// Reuse existing test runner image instead of rebuilding (useful in CI)
    #[arg(long)]
    reuse_image: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::stop::exec(
            ComposeTestLocalConfig::integration(),
            &self.integration,
            self.build_all,
            self.reuse_image,
        )
    }
}
