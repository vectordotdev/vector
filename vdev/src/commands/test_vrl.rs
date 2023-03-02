use anyhow::{Context as _, Result};

use crate::{app};
use std::{env, path::PathBuf};

/// Run the Vector Remap Language test suite
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let path: PathBuf = [app::path(), "lib", "vrl", "tests"].into_iter().collect();
        env::set_current_dir(path).context("Could not change directory")?;

        #[allow(clippy::case_sensitive_file_extension_comparisons)]
        app::exec(
            "cargo",
            ["run", "--", "--runtime=ast"].into_iter(),
            false,
        )
    }
}
