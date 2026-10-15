use crate::app;
use crate::app::CommandExt;
use crate::utils::git;
use anyhow::{Result, bail};
use std::process::Command;

#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

const CUE_SCRIPT: &str = "scripts/cue.sh";
const CUE_DIR: &str = "website/cue";

/// Checks that the contents of the /website/cue folder are valid. This includes:
///
/// 1. Ensuring that the .cue files are properly formatted.
/// 2. Ensuring that the .cue files can compile.
impl Cli {
    pub fn exec(self) -> Result<()> {
        if Command::new("cue").arg("version").check_output().is_err() {
            bail!("Error: cue is not installed");
        }

        println!("Validating cue files formatting...");

        let unformatted_files = Self::get_unformatted_cue_files()?;

        if !unformatted_files.is_empty() {
            bail!(
                "Incorrectly formatted CUE files:\n\n{}\n\nRun './{} fmt' to fix formatting issues.",
                unformatted_files.join("\n"),
                CUE_SCRIPT
            );
        }

        println!("Validating cue files correctness...");

        if let Err(err) = app::exec(CUE_SCRIPT, ["vet"], true) {
            bail!("CUE files validation failed:\n{err}");
        }

        println!("Success! The contents of the sources in the './{CUE_DIR}' directory are valid");
        Ok(())
    }

    fn get_unformatted_cue_files() -> Result<Vec<String>> {
        app::exec(CUE_SCRIPT, ["fmt"], true)?;

        let file_names = git::run_and_check_output(&["diff", "--name-only", CUE_DIR])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect();

        Ok(file_names)
    }
}
