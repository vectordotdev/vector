use crate::app;
use crate::app::CommandExt;
use crate::utils::{git, paths};
use anyhow::{Context as _, Result, bail};
use glob::glob;
use std::env;
use std::path::PathBuf;
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
        // Resolve all `website/cue/**` globs against the repo root rather
        // than the caller's CWD. Without this, running the binary from
        // anywhere outside the repo root silently scans nothing and reports `0 error(s)`
        let repo_root = paths::find_repo_root()?;
        env::set_current_dir(&repo_root)?;

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

        let all_cue_files = Self::get_all_cue_files()?;

        Command::new("cue")
            .args(["vet", "--concrete", "--all-errors"])
            .args(all_cue_files)
            .check_output()
            .context("CUE files validation failed")?;

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

    fn get_all_cue_files() -> Result<Vec<PathBuf>> {
        let pattern = &format!("{CUE_DIR}/**/*.cue");

        // Paths are yielded in alphabetical order via glob
        let files: Vec<PathBuf> = glob(pattern)?.collect::<Result<Vec<PathBuf>, _>>()?;

        Ok(files)
    }
}
