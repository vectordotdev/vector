use anyhow::Result;
use clap::Args;

use crate::testing::docker::CONTAINER_TOOL;
use crate::{app, config, platform};

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

        println!("\nConfig:");
        match config::path() {
            Ok(path) => {
                println!("  Path:        {}", path.display());
                match config::load() {
                    Ok(config) => {
                        println!("  Repository:  {:?}", config.repo);
                    }
                    Err(error) => println!("  Could not load: {error}"),
                }
            }
            Err(error) => println!("  Path:  Not found: {error}"),
        }

        println!("\nPlatform:");
        println!("  Default target:  {}", platform::default_target());
        Ok(())
    }
}
