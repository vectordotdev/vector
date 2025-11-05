use crate::testing::build::{ALL_E2E_FEATURE_FLAG, build_test_runner_image};
use crate::testing::integration::ComposeTestKind;
use anyhow::Result;
use clap::Args;

/// Build the E2E test runner image with all E2E features
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        build_test_runner_image(
            ComposeTestKind::E2E,
            ALL_E2E_FEATURE_FLAG,
            true, // E2E tests pre-build test binaries and Vector.
        )
    }
}
