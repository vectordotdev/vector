use anyhow::Result;
use clap::Args;

use crate::testing::integration::E2ETest;

/// Stop an e2e-test environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The e2e test name to stop
    test: String,

    /// If true, remove the runner container compiled with all integration test features
    #[arg(short = 'a', long)]
    all_features: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::stop::exec::<E2ETest>(&self.test, self.all_features)
    }
}
