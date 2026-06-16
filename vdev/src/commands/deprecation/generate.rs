#![allow(clippy::print_stdout)]

use anyhow::{Result, bail};

use crate::utils::{deprecation, paths};

/// Regenerate generated/deprecations.json from deprecation.d/ fragments
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = paths::find_repo_root()?;
        let dir = repo_root.join(deprecation::DEPRECATION_DIR);
        if !dir.is_dir() {
            bail!(
                "{} not found; the deprecation fragment system is not installed in this repo.",
                dir.display()
            );
        }
        deprecation::sync_deprecations_cue(&repo_root)?;
        println!(
            "Updated {}",
            repo_root.join(deprecation::DEPRECATIONS_JSON).display()
        );
        Ok(())
    }
}
