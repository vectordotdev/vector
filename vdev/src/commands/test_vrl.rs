use anyhow::{Context as _, Result};

use crate::app;
use std::{env, path::PathBuf};

/// Run the Vector Remap Language test suite
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        run_tests(&[app::path(), "lib", "vector-vrl", "tests"])?;
        Ok(())
    }
}

fn run_tests(path: &[&str]) -> Result<()> {
    let path: PathBuf = path.iter().collect();
    env::set_current_dir(path).context("Could not change directory")?;
    app::exec("cargo", ["run"], false)
}
