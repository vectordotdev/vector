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
        let json_path = repo_root.join(deprecation::DEPRECATIONS_JSON);
        // If neither input exists, the fragment system isn't installed. With
        // either present, we can still produce or refresh the JSON (an empty
        // pending list is valid once all fragments have been enacted).
        if !dir.is_dir() && !json_path.is_file() {
            bail!(
                "Neither {} nor {} found; the deprecation fragment system is not installed in this repo.",
                dir.display(),
                json_path.display()
            );
        }
        deprecation::sync_deprecations_cue(&repo_root)?;
        println!("Updated {}", json_path.display());
        Ok(())
    }
}
