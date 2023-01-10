use std::{env, path::Path};

use anyhow::{Context as _, Result};

use crate::app;

/// Check that all files are formatted properly
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        env::set_current_dir(app::path()).context("Could not change directory")?;
        app::exec(Path::new("cargo"), ["fmt", "--", "--check"])
    }
}
