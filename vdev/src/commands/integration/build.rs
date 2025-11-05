use crate::testing::build::{ALL_INTEGRATIONS_FEATURE_FLAG, build_test_runner_image};
use crate::testing::integration::ComposeTestKind;
use anyhow::Result;
use clap::Args;

/// Build the integration test runner image with all integration features
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        build_test_runner_image(
            ComposeTestKind::Integration,
            ALL_INTEGRATIONS_FEATURE_FLAG,
            true, // Integration tests pre-build test binaries.
        )
    }
}
