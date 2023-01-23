use anyhow::Result;
use clap::Args;

use crate::{app, config, platform, testing::runner};

/// Show `vdev` command configuration
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        display!("Container tool:  {:?}", *runner::CONTAINER_TOOL);
        display!("Data path:       {:?}", platform::data_dir());
        display!("Repository:      {:?}", app::path());
        display!("Shell:           {:?}", *app::SHELL);

        display!("\nConfig:");
        match config::path() {
            Ok(path) => {
                display!("  Path:        {path:?}");
                match config::load() {
                    Ok(config) => {
                        display!("  Repository:  {:?}", config.repo);
                    }
                    Err(error) => display!("  Could not load: {error}"),
                }
            }
            Err(error) => display!("  Path:  Not found: {error}"),
        }

        display!("\nPlatform:");
        display!("  Default target:  {}", platform::default_target());
        Ok(())
    }
}
