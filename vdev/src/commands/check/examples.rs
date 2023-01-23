use std::fs;

use anyhow::{bail, Context, Result};

use crate::app;

/// Check that the config/example files are valid
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

const EXAMPLES: &str = "config/examples";

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::set_repo_dir()?;

        for entry in fs::read_dir(EXAMPLES)
            .with_context(|| format!("Could not read directory {EXAMPLES:?}"))?
        {
            let entry = entry.with_context(|| format!("Could not read entry from {EXAMPLES:?}"))?;
            let filename = entry.path();
            let Some(filename) = filename.as_os_str().to_str() else {
                bail!("Invalid filename {filename:?}");
            };

            let mut command = vec![
                "run",
                "--",
                "validate",
                "--deny-warnings",
                "--no-environment",
            ];
            if entry
                .metadata()
                .with_context(|| format!("Could not get metadata of {filename:?}"))?
                .is_dir()
            {
                command.push("--config-dir");
            }
            command.push(filename);

            app::exec("cargo", command, true)?;
        }

        Ok(())
    }
}
