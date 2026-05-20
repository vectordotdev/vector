use anyhow::Result;
use clap::Args;

use crate::{app, testing::docker::CONTAINER_TOOL, utils::platform};

/// Show `vdev` command configuration
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        println!("Container tool:  {}", CONTAINER_TOOL.display());
        println!("Data path:       {}", platform::data_dir().display());
        println!("Repository:      {:?}", app::path());
        println!("Shell:           {}", app::SHELL.display());

        println!("\nPlatform:");
        println!("  Default target:  {}", platform::default_target());
        Ok(())
    }
}
