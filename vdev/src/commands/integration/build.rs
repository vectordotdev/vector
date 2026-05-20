use crate::testing::build::build_integration_image;
use anyhow::Result;
use clap::Args;

/// Start an environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        build_integration_image()
    }
}
