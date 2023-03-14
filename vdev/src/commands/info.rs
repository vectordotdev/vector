use anyhow::Result;
use clap::Args;

use crate::{app, config, platform, testing::runner};

/// Show `vdev` command configuration
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        println!("Container tool:  {:?}", *runner::CONTAINER_TOOL);
        println!("Data path:       {:?}", platform::data_dir());
        println!("Repository:      {:?}", app::path());
        println!("Shell:           {:?}", *app::SHELL);

        println!("\nConfig:");
        match config::path() {
            Ok(path) => {
                println!("  Path:        {path:?}");
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
