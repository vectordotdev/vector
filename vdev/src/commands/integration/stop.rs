use anyhow::Result;
use clap::Args;

use crate::testing::integration::IntegrationTest;

/// Stop an integration test environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The integration name to stop
    integration: String,

    /// If true, remove the runner container compiled with all integration test features
    #[arg(short = 'a', long)]
    all_features: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::stop::exec::<IntegrationTest>(
            &self.integration,
            self.all_features,
        )
    }
}
